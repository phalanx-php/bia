use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use bumpalo::Bump;
use mago_database::file::{File, FileType};
use mago_span::{HasSpan, Span};
use mago_syntax::ast::{
    ClassLikeMember, Constant, Enum, Function, Identifier, LocalIdentifier, Method, Namespace,
    Program, Property, PropertyItem, Statement, Trait,
};
use mago_syntax::lexer::Lexer;
use mago_syntax::parser::parse_file;
use mago_syntax::settings::LexerSettings;
use mago_syntax_core::input::Input;
use serde::{Deserialize, Serialize};

const INLINE_PREFIX: &str = "<?php\n";
const DEFAULT_INLINE_NAME: &str = "dory://inline.php";

static PROJECT_CACHE: OnceLock<Mutex<HashMap<String, IndexedProject>>> = OnceLock::new();

struct AnalysisSource {
    file: File,
    map: SourceMap,
}

struct SourceMap {
    original: Vec<u8>,
    lines: Vec<u32>,
    prefix_len: u32,
    wrapped: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SourceFileRecord {
    id: String,
    name: String,
    wrapped: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SpanRecord {
    start_offset: u32,
    end_offset: u32,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ParseErrorRecord {
    message: String,
    span: SpanRecord,
}

#[derive(Clone, Debug, Serialize)]
struct TokenRecord {
    kind: String,
    text: String,
    span: SpanRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct DeclarationRecord {
    kind: &'static str,
    name: String,
    namespace: Option<String>,
    declaring_type: Option<String>,
    fqn: String,
    span: SpanRecord,
    name_span: SpanRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
}

#[derive(Debug, Serialize)]
struct ParsePayload {
    ok: bool,
    file: SourceFileRecord,
    has_errors: bool,
    errors: Vec<ParseErrorRecord>,
    tokens: Vec<TokenRecord>,
    declarations: Vec<DeclarationRecord>,
}

#[derive(Debug, Serialize)]
struct ErrorPayload {
    ok: bool,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum CodeQueryRequest {
    ParseSource {
        source: String,
        name: Option<String>,
    },
    ParseFile {
        path: String,
    },
    IndexProject {
        root: String,
    },
    QueryDeclarations {
        root: String,
        #[serde(default)]
        query: DeclarationQuery,
    },
    QueryTokens {
        root: String,
        #[serde(default)]
        query: TokenQuery,
    },
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DeclarationQuery {
    kind: Option<String>,
    name: Option<String>,
    fqn: Option<String>,
    file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TokenQuery {
    kind: Option<String>,
    text: Option<String>,
    file: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileFingerprint {
    path: String,
    len: u64,
    modified_ns: u128,
}

#[derive(Clone, Debug)]
struct ProjectFile {
    absolute: PathBuf,
    relative: String,
    fingerprint: FileFingerprint,
}

#[derive(Clone, Debug)]
struct IndexedProject {
    root: String,
    fingerprint: Vec<FileFingerprint>,
    files: Vec<SourceFileRecord>,
    declarations: Vec<DeclarationRecord>,
    tokens: Vec<TokenRecord>,
    errors: Vec<ParseErrorRecord>,
}

#[derive(Debug, Serialize)]
struct ProjectIndexPayload {
    ok: bool,
    root: String,
    files: Vec<SourceFileRecord>,
    file_count: usize,
    declaration_count: usize,
    token_count: usize,
    errors: Vec<ParseErrorRecord>,
}

#[derive(Debug, Serialize)]
struct DeclarationQueryPayload {
    ok: bool,
    root: String,
    declarations: Vec<DeclarationRecord>,
    errors: Vec<ParseErrorRecord>,
}

#[derive(Debug, Serialize)]
struct TokenQueryPayload {
    ok: bool,
    root: String,
    tokens: Vec<TokenRecord>,
    errors: Vec<ParseErrorRecord>,
}

#[derive(Clone, Default)]
struct DeclarationContext {
    namespace: Option<String>,
    declaring_type: Option<String>,
}

pub fn error_json(message: impl Into<String>) -> String {
    serde_json::to_string(&ErrorPayload {
        ok: false,
        message: message.into(),
    })
    .unwrap_or_else(|_| "{\"ok\":false,\"message\":\"failed to encode error\"}".to_string())
}

pub fn dispatch_query_json(request_json: &str) -> String {
    let request = match serde_json::from_str::<CodeQueryRequest>(request_json) {
        Ok(request) => request,
        Err(error) => return error_json(format!("invalid Dory code query request: {error}")),
    };

    match request {
        CodeQueryRequest::ParseSource { source, name } => {
            parse_source_json(&source, name.as_deref())
        }
        CodeQueryRequest::ParseFile { path } => parse_file_json(&path),
        CodeQueryRequest::IndexProject { root } => project_index_json(&root),
        CodeQueryRequest::QueryDeclarations { root, query } => {
            declaration_query_json(&root, &query)
        }
        CodeQueryRequest::QueryTokens { root, query } => token_query_json(&root, &query),
    }
}

pub fn parse_source_json(source: &str, name: Option<&str>) -> String {
    let name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_INLINE_NAME);
    let (source, map) = SourceMap::inline(source);
    let file = File::ephemeral(
        Cow::Owned(name.as_bytes().to_vec()),
        Cow::Owned(source.into_bytes()),
    );
    let source = AnalysisSource { file, map };

    parse_file_payload(&source)
}

pub fn parse_file_json(path: &str) -> String {
    let path = Path::new(path);
    let workspace = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let file = match File::read(&workspace, path, FileType::Host) {
        Ok(file) => file,
        Err(error) => {
            return error_json(format!("failed to read PHP source file: {error}"));
        }
    };
    let source = AnalysisSource {
        map: SourceMap::file(&file),
        file,
    };

    parse_file_payload(&source)
}

fn project_index_json(root: &str) -> String {
    match project_index(root) {
        Ok(index) => encode_json(&ProjectIndexPayload {
            ok: true,
            root: index.root,
            file_count: index.files.len(),
            declaration_count: index.declarations.len(),
            token_count: index.tokens.len(),
            files: index.files,
            errors: index.errors,
        }),
        Err(error) => error_json(error),
    }
}

fn declaration_query_json(root: &str, query: &DeclarationQuery) -> String {
    match project_index(root) {
        Ok(index) => {
            let declarations = index
                .declarations
                .into_iter()
                .filter(|declaration| query.matches(declaration))
                .collect();

            encode_json(&DeclarationQueryPayload {
                ok: true,
                root: index.root,
                declarations,
                errors: index.errors,
            })
        }
        Err(error) => error_json(error),
    }
}

fn token_query_json(root: &str, query: &TokenQuery) -> String {
    match project_index(root) {
        Ok(index) => {
            let tokens = index
                .tokens
                .into_iter()
                .filter(|token| query.matches(token))
                .collect();

            encode_json(&TokenQueryPayload {
                ok: true,
                root: index.root,
                tokens,
                errors: index.errors,
            })
        }
        Err(error) => error_json(error),
    }
}

fn parse_file_payload(source: &AnalysisSource) -> String {
    encode_json(&parse_file_payload_record(source))
}

fn parse_file_payload_record(source: &AnalysisSource) -> ParsePayload {
    let arena = Bump::new();
    let file = &source.file;
    let program = parse_file(&arena, file);
    let (tokens, lexer_errors) = collect_tokens(source);
    let errors = collect_errors(source, program)
        .into_iter()
        .chain(lexer_errors)
        .collect();
    let declarations = collect_declarations(source, program);

    ParsePayload {
        ok: true,
        file: SourceFileRecord {
            id: file.id.as_u64().to_string(),
            name: String::from_utf8_lossy(&file.name).into_owned(),
            wrapped: source.map.wrapped,
        },
        has_errors: program.has_errors() || !errors.is_empty(),
        errors,
        tokens,
        declarations,
    }
}

fn encode_json<T>(payload: &T) -> String
where
    T: Serialize,
{
    serde_json::to_string(payload)
        .unwrap_or_else(|error| error_json(format!("failed to encode code query result: {error}")))
}

impl SourceMap {
    fn inline(source: &str) -> (String, Self) {
        let trimmed = source.trim_start();
        if trimmed.starts_with("<?php") || trimmed.starts_with("<?=") || trimmed.starts_with("<? ")
        {
            return (
                source.to_string(),
                Self::new(source.as_bytes().to_vec(), 0, false),
            );
        }

        (
            format!("{INLINE_PREFIX}{source}"),
            Self::new(source.as_bytes().to_vec(), INLINE_PREFIX.len() as u32, true),
        )
    }

    fn file(file: &File) -> Self {
        Self::new(file.contents.as_ref().to_vec(), 0, false)
    }

    fn new(original: Vec<u8>, prefix_len: u32, wrapped: bool) -> Self {
        let mut lines = vec![0];

        for (offset, byte) in original.iter().enumerate() {
            if *byte == b'\n' {
                lines.push((offset + 1) as u32);
            }
        }

        Self {
            original,
            lines,
            prefix_len,
            wrapped,
        }
    }

    fn contains_source_span(&self, span: Span) -> bool {
        !self.wrapped || span.end.offset > self.prefix_len
    }

    fn span_record(&self, span: Span) -> SpanRecord {
        let start_offset = self.source_offset(span.start.offset);
        let end_offset = self.source_offset(span.end.offset);

        SpanRecord {
            start_offset,
            end_offset,
            start_line: self.line_number(start_offset) + 1,
            start_column: self.column_number(start_offset) + 1,
            end_line: self.line_number(end_offset) + 1,
            end_column: self.column_number(end_offset) + 1,
        }
    }

    fn source_offset(&self, parsed_offset: u32) -> u32 {
        if self.wrapped {
            parsed_offset.saturating_sub(self.prefix_len)
        } else {
            parsed_offset
        }
    }

    fn line_number(&self, offset: u32) -> u32 {
        let offset = offset.min(self.original.len() as u32);

        self.lines
            .binary_search(&offset)
            .unwrap_or_else(|next_line| next_line.saturating_sub(1)) as u32
    }

    fn column_number(&self, offset: u32) -> u32 {
        let offset = offset.min(self.original.len() as u32);
        let line = self.line_number(offset) as usize;

        offset - self.lines[line]
    }
}

impl DeclarationQuery {
    fn matches(&self, declaration: &DeclarationRecord) -> bool {
        self.kind
            .as_deref()
            .is_none_or(|kind| declaration.kind == kind)
            && self
                .name
                .as_deref()
                .is_none_or(|name| declaration.name == name)
            && self.fqn.as_deref().is_none_or(|fqn| declaration.fqn == fqn)
            && self.file.as_deref().is_none_or(|file| {
                declaration
                    .file
                    .as_deref()
                    .is_some_and(|declaration_file| declaration_file == file)
            })
    }
}

impl TokenQuery {
    fn matches(&self, token: &TokenRecord) -> bool {
        self.kind.as_deref().is_none_or(|kind| token.kind == kind)
            && self.text.as_deref().is_none_or(|text| token.text == text)
            && self.file.as_deref().is_none_or(|file| {
                token
                    .file
                    .as_deref()
                    .is_some_and(|token_file| token_file == file)
            })
    }
}

fn project_index(root: &str) -> Result<IndexedProject, String> {
    let root_path = normalize_root(root)?;
    let root_key = root_path.to_string_lossy().into_owned();
    let files = collect_project_files(&root_path)?;
    let fingerprint = files
        .iter()
        .map(|file| file.fingerprint.clone())
        .collect::<Vec<_>>();

    let cache = PROJECT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(index) = cache.get(&root_key) {
            if index.fingerprint == fingerprint {
                return Ok(index.clone());
            }
        }
    }

    let index = build_project_index(root_key.clone(), files, fingerprint);

    if let Ok(mut cache) = cache.lock() {
        cache.insert(root_key, index.clone());
    }

    Ok(index)
}

fn normalize_root(root: &str) -> Result<PathBuf, String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("project root must not be empty".to_string());
    }

    let path = Path::new(root);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(path)
    };

    let path = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve project root: {error}"))?;

    if !path.is_dir() {
        return Err("project root must be a directory".to_string());
    }

    Ok(path)
}

fn collect_project_files(root: &Path) -> Result<Vec<ProjectFile>, String> {
    let mut files = Vec::new();
    collect_project_files_into(root, root, &mut files)?;
    files.sort_by(|left, right| left.relative.cmp(&right.relative));

    Ok(files)
}

fn collect_project_files_into(
    root: &Path,
    directory: &Path,
    files: &mut Vec<ProjectFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read project directory: {error}"))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read project entry: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect project entry: {error}"))?;

        if file_type.is_dir() {
            if !is_excluded_directory(&entry.file_name().to_string_lossy()) {
                collect_project_files_into(root, &path, files)?;
            }

            continue;
        }

        if !file_type.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("php") {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|error| format!("failed to inspect PHP source file: {error}"))?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());

        files.push(ProjectFile {
            absolute: path,
            relative: relative.clone(),
            fingerprint: FileFingerprint {
                path: relative,
                len: metadata.len(),
                modified_ns,
            },
        });
    }

    Ok(())
}

fn is_excluded_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".aimind"
            | ".claude"
            | ".daemon8"
            | "build"
            | "dist"
            | "node_modules"
            | "storage"
            | "target"
            | "var"
            | "vendor"
    )
}

fn build_project_index(
    root: String,
    files: Vec<ProjectFile>,
    fingerprint: Vec<FileFingerprint>,
) -> IndexedProject {
    let mut indexed = IndexedProject {
        root,
        fingerprint,
        files: Vec::new(),
        declarations: Vec::new(),
        tokens: Vec::new(),
        errors: Vec::new(),
    };

    for file in files {
        let contents = match fs::read(&file.absolute) {
            Ok(contents) => contents,
            Err(error) => {
                indexed.errors.push(file_error(
                    &file.relative,
                    format!("failed to read PHP source file: {error}"),
                ));
                continue;
            }
        };
        let source = AnalysisSource {
            map: SourceMap::new(contents.clone(), 0, false),
            file: File::ephemeral(
                Cow::Owned(file.relative.as_bytes().to_vec()),
                Cow::Owned(contents),
            ),
        };
        let payload = parse_file_payload_record(&source);

        indexed.files.push(payload.file);
        indexed.errors.extend(
            payload
                .errors
                .into_iter()
                .map(|error| error.with_file(&file.relative)),
        );
        indexed.tokens.extend(
            payload
                .tokens
                .into_iter()
                .map(|token| token.with_file(&file.relative)),
        );
        indexed.declarations.extend(
            payload
                .declarations
                .into_iter()
                .map(|declaration| declaration.with_file(&file.relative)),
        );
    }

    indexed
}

fn file_error(file: &str, message: String) -> ParseErrorRecord {
    ParseErrorRecord {
        message: format!("{file}: {message}"),
        span: SpanRecord {
            start_offset: 0,
            end_offset: 0,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
    }
}

impl ParseErrorRecord {
    fn with_file(mut self, file: &str) -> Self {
        self.message = format!("{file}: {}", self.message);
        self
    }
}

impl TokenRecord {
    fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}

impl DeclarationRecord {
    fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}

fn collect_errors(source: &AnalysisSource, program: &Program<'_>) -> Vec<ParseErrorRecord> {
    program
        .errors
        .iter()
        .map(|error| ParseErrorRecord {
            message: error.to_string(),
            span: source.map.span_record(error.span()),
        })
        .collect()
}

fn collect_tokens(source: &AnalysisSource) -> (Vec<TokenRecord>, Vec<ParseErrorRecord>) {
    let file = &source.file;
    let input = Input::new(file.id, file.contents.as_ref());
    let mut lexer = Lexer::new(input, LexerSettings::default());
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    while let Some(token) = lexer.advance() {
        let token = match token {
            Ok(token) => token,
            Err(error) => {
                errors.push(ParseErrorRecord {
                    message: error.to_string(),
                    span: source.map.span_record(error.span()),
                });

                continue;
            }
        };

        let span = token.span_for(file.id);
        if !source.map.contains_source_span(span) {
            continue;
        }

        tokens.push(TokenRecord {
            kind: token.kind.to_string(),
            text: String::from_utf8_lossy(token.value).into_owned(),
            span: source.map.span_record(span),
            file: None,
        });
    }

    (tokens, errors)
}

fn collect_declarations(source: &AnalysisSource, program: &Program<'_>) -> Vec<DeclarationRecord> {
    let mut declarations = Vec::new();
    collect_statement_declarations(
        source,
        &DeclarationContext::default(),
        program.statements.as_slice(),
        &mut declarations,
    );
    declarations
}

fn collect_statement_declarations(
    source: &AnalysisSource,
    context: &DeclarationContext,
    statements: &[Statement<'_>],
    declarations: &mut Vec<DeclarationRecord>,
) {
    for statement in statements {
        match statement {
            Statement::Namespace(namespace) => {
                collect_namespace_declarations(source, namespace, declarations)
            }
            Statement::Class(class) => {
                let name = local_name(&class.name);
                let fqn = qualified_name(context.namespace.as_deref(), &name);
                declarations.push(declaration(
                    source,
                    "class",
                    &name,
                    context.namespace.clone(),
                    None,
                    fqn.clone(),
                    class.span(),
                    class.name.span(),
                ));
                collect_member_declarations(
                    source,
                    &context.for_type(fqn),
                    class.members.as_slice(),
                    declarations,
                );
            }
            Statement::Interface(interface) => {
                let name = local_name(&interface.name);
                let fqn = qualified_name(context.namespace.as_deref(), &name);
                declarations.push(declaration(
                    source,
                    "interface",
                    &name,
                    context.namespace.clone(),
                    None,
                    fqn.clone(),
                    interface.span(),
                    interface.name.span(),
                ));
                collect_member_declarations(
                    source,
                    &context.for_type(fqn),
                    interface.members.as_slice(),
                    declarations,
                );
            }
            Statement::Trait(r#trait) => {
                collect_trait_declarations(source, r#trait, context, declarations)
            }
            Statement::Enum(r#enum) => {
                collect_enum_declarations(source, r#enum, context, declarations)
            }
            Statement::Function(function) => {
                collect_function_declaration(source, function, context, declarations)
            }
            Statement::Constant(constant) => {
                collect_constant_declarations(source, constant, context, declarations)
            }
            _ => {}
        }
    }
}

fn collect_namespace_declarations(
    source: &AnalysisSource,
    namespace: &Namespace<'_>,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let context = DeclarationContext {
        namespace: namespace.name.as_ref().map(identifier_name),
        declaring_type: None,
    };

    collect_statement_declarations(
        source,
        &context,
        namespace.statements().as_slice(),
        declarations,
    );
}

fn collect_trait_declarations(
    source: &AnalysisSource,
    r#trait: &Trait<'_>,
    context: &DeclarationContext,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let name = local_name(&r#trait.name);
    let fqn = qualified_name(context.namespace.as_deref(), &name);
    declarations.push(declaration(
        source,
        "trait",
        &name,
        context.namespace.clone(),
        None,
        fqn.clone(),
        r#trait.span(),
        r#trait.name.span(),
    ));
    collect_member_declarations(
        source,
        &context.for_type(fqn),
        r#trait.members.as_slice(),
        declarations,
    );
}

fn collect_enum_declarations(
    source: &AnalysisSource,
    r#enum: &Enum<'_>,
    context: &DeclarationContext,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let name = local_name(&r#enum.name);
    let fqn = qualified_name(context.namespace.as_deref(), &name);
    declarations.push(declaration(
        source,
        "enum",
        &name,
        context.namespace.clone(),
        None,
        fqn.clone(),
        r#enum.span(),
        r#enum.name.span(),
    ));
    collect_member_declarations(
        source,
        &context.for_type(fqn),
        r#enum.members.as_slice(),
        declarations,
    );
}

fn collect_function_declaration(
    source: &AnalysisSource,
    function: &Function<'_>,
    context: &DeclarationContext,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let name = local_name(&function.name);
    declarations.push(declaration(
        source,
        "function",
        &name,
        context.namespace.clone(),
        None,
        qualified_name(context.namespace.as_deref(), &name),
        function.span(),
        function.name.span(),
    ));
}

fn collect_constant_declarations(
    source: &AnalysisSource,
    constant: &Constant<'_>,
    context: &DeclarationContext,
    declarations: &mut Vec<DeclarationRecord>,
) {
    for item in &constant.items {
        let name = local_name(&item.name);
        declarations.push(declaration(
            source,
            "constant",
            &name,
            context.namespace.clone(),
            None,
            qualified_name(context.namespace.as_deref(), &name),
            item.span(),
            item.name.span(),
        ));
    }
}

fn collect_member_declarations(
    source: &AnalysisSource,
    context: &DeclarationContext,
    members: &[ClassLikeMember<'_>],
    declarations: &mut Vec<DeclarationRecord>,
) {
    let Some(declaring_type) = context.declaring_type.as_deref() else {
        return;
    };

    for member in members {
        match member {
            ClassLikeMember::Method(method) => {
                collect_method_declaration(source, context, method, declarations)
            }
            ClassLikeMember::Property(property) => {
                collect_property_declarations(source, context, property, declarations)
            }
            ClassLikeMember::Constant(constant) => {
                for item in &constant.items {
                    let name = local_name(&item.name);
                    declarations.push(declaration(
                        source,
                        "class-constant",
                        &name,
                        context.namespace.clone(),
                        Some(declaring_type.to_string()),
                        member_name(declaring_type, &name),
                        item.span(),
                        item.name.span(),
                    ));
                }
            }
            _ => {}
        }
    }
}

fn collect_method_declaration(
    source: &AnalysisSource,
    context: &DeclarationContext,
    method: &Method<'_>,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let Some(declaring_type) = context.declaring_type.as_deref() else {
        return;
    };
    let name = local_name(&method.name);

    declarations.push(declaration(
        source,
        "method",
        &name,
        context.namespace.clone(),
        Some(declaring_type.to_string()),
        member_name(declaring_type, &name),
        method.span(),
        method.name.span(),
    ));
}

fn collect_property_declarations(
    source: &AnalysisSource,
    context: &DeclarationContext,
    property: &Property<'_>,
    declarations: &mut Vec<DeclarationRecord>,
) {
    match property {
        Property::Plain(plain) => {
            for item in &plain.items {
                collect_property_item(source, context, item, declarations);
            }
        }
        Property::Hooked(hooked) => {
            collect_property_item(source, context, &hooked.item, declarations)
        }
    }
}

fn collect_property_item(
    source: &AnalysisSource,
    context: &DeclarationContext,
    item: &PropertyItem<'_>,
    declarations: &mut Vec<DeclarationRecord>,
) {
    let Some(declaring_type) = context.declaring_type.as_deref() else {
        return;
    };

    let variable = match item {
        PropertyItem::Abstract(item) => &item.variable,
        PropertyItem::Concrete(item) => &item.variable,
    };
    let name = String::from_utf8_lossy(variable.name).into_owned();

    declarations.push(declaration(
        source,
        "property",
        &name,
        context.namespace.clone(),
        Some(declaring_type.to_string()),
        member_name(declaring_type, &name),
        item.span(),
        variable.span(),
    ));
}

fn declaration(
    source: &AnalysisSource,
    kind: &'static str,
    name: &str,
    namespace: Option<String>,
    declaring_type: Option<String>,
    fqn: String,
    span: Span,
    name_span: Span,
) -> DeclarationRecord {
    DeclarationRecord {
        kind,
        name: name.to_string(),
        namespace,
        declaring_type,
        fqn,
        span: source.map.span_record(span),
        name_span: source.map.span_record(name_span),
        file: None,
    }
}

fn local_name(identifier: &LocalIdentifier<'_>) -> String {
    String::from_utf8_lossy(identifier.value).into_owned()
}

fn identifier_name(identifier: &Identifier<'_>) -> String {
    String::from_utf8_lossy(identifier.value()).into_owned()
}

fn qualified_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) if !namespace.is_empty() => format!("{namespace}\\{name}"),
        _ => name.to_string(),
    }
}

fn member_name(declaring_type: &str, name: &str) -> String {
    format!("{declaring_type}::{name}")
}

impl DeclarationContext {
    fn for_type(&self, declaring_type: String) -> Self {
        Self {
            namespace: self.namespace.clone(),
            declaring_type: Some(declaring_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn parses_bare_php_source() {
        let json = parse_source_json(
            "class Demo { public function run(): void {} }",
            Some("demo.php"),
        );

        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"kind\":\"class\""));
        assert!(json.contains("\"name\":\"Demo\""));
        assert!(json.contains("\"kind\":\"method\""));
        assert!(json.contains("\"name\":\"run\""));
    }

    #[test]
    fn reports_parse_errors() {
        let json = parse_source_json("class {", Some("broken.php"));

        assert!(json.contains("\"has_errors\":true"));
        assert!(json.contains("\"errors\":["));
    }

    #[test]
    fn keeps_tagged_source_unwrapped() {
        let json = parse_source_json("<?php function demo() {}", Some("tagged.php"));

        assert!(json.contains("\"wrapped\":false"));
        assert!(json.contains("\"kind\":\"function\""));
    }

    #[test]
    fn maps_bare_source_spans_to_original_snippet() {
        let json = parse_source_json("class Demo {}", Some("demo.php"));
        let payload: Value = serde_json::from_str(&json).expect("valid parse payload");

        assert_eq!(payload["file"]["wrapped"], true);
        assert_eq!(payload["tokens"][0]["text"], "class");
        assert_eq!(payload["tokens"][0]["span"]["start_offset"], 0);
        assert_eq!(payload["declarations"][0]["name"], "Demo");
        assert_eq!(payload["declarations"][0]["name_span"]["start_offset"], 6);
    }

    #[test]
    fn records_qualified_declaration_identity() {
        let json = parse_source_json(
            "namespace App\\Domain; class Demo { public function run(): void {} }",
            Some("demo.php"),
        );
        let payload: Value = serde_json::from_str(&json).expect("valid parse payload");

        assert_eq!(payload["declarations"][0]["kind"], "class");
        assert_eq!(payload["declarations"][0]["namespace"], "App\\Domain");
        assert_eq!(payload["declarations"][0]["declaring_type"], Value::Null);
        assert_eq!(payload["declarations"][0]["fqn"], "App\\Domain\\Demo");
        assert_eq!(payload["declarations"][1]["kind"], "method");
        assert_eq!(payload["declarations"][1]["namespace"], "App\\Domain");
        assert_eq!(
            payload["declarations"][1]["declaring_type"],
            "App\\Domain\\Demo"
        );
        assert_eq!(payload["declarations"][1]["fqn"], "App\\Domain\\Demo::run");
    }

    #[test]
    fn dispatches_parse_source_queries() {
        let json = dispatch_query_json(
            r#"{"op":"parse_source","source":"class Demo {}","name":"demo.php"}"#,
        );
        let payload: Value = serde_json::from_str(&json).expect("valid parse payload");

        assert_eq!(payload["ok"], true);
        assert_eq!(payload["file"]["name"], "demo.php");
        assert_eq!(payload["declarations"][0]["name"], "Demo");
    }

    #[test]
    fn indexes_and_queries_project_php_files() {
        let root = std::env::temp_dir().join(format!("dory-analysis-test-{}", std::process::id()));
        let src = root.join("src");

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&src).expect("create test project");
        fs::write(
            src.join("Example.php"),
            "<?php namespace App; final class Example { public function run(): void {} }",
        )
        .expect("write PHP fixture");
        fs::create_dir_all(root.join("vendor")).expect("create excluded directory");
        fs::write(
            root.join("vendor").join("Ignored.php"),
            "<?php class Ignored {}",
        )
        .expect("write ignored PHP fixture");

        let request = serde_json::json!({
            "op": "query_declarations",
            "root": root,
            "query": {"kind": "class", "name": "Example"}
        });
        let json = dispatch_query_json(&request.to_string());
        let payload: Value = serde_json::from_str(&json).expect("valid query payload");

        assert_eq!(payload["ok"], true);
        assert_eq!(payload["declarations"].as_array().unwrap().len(), 1);
        assert_eq!(payload["declarations"][0]["file"], "src/Example.php");
        assert_eq!(payload["declarations"][0]["fqn"], "App\\Example");

        let _ = fs::remove_dir_all(root);
    }
}
