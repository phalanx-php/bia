use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

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
use serde::Serialize;

mod engine;
mod project;
mod query;
mod records;
mod request;

#[cfg(test)]
pub use engine::dispatch_query_json;
pub use engine::{error_json, CodeQueryEngine};
use records::{
    DeclarationRecord, ParseErrorRecord, ParsePayload, SourceFileRecord, SpanRecord, TokenRecord,
};

const INLINE_PREFIX: &str = "<?php\n";
const DEFAULT_INLINE_NAME: &str = "dory://inline.php";

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

#[derive(Clone, Default)]
struct DeclarationContext {
    namespace: Option<String>,
    declaring_type: Option<String>,
}

#[cfg(test)]
fn parse_source_json(source: &str, name: Option<&str>) -> String {
    parse_source_bytes_json(source.as_bytes(), name)
}

#[cfg(test)]
fn parse_source_bytes_json(source: &[u8], name: Option<&str>) -> String {
    encode_json(&parse_source_bytes(source, name))
}

pub(crate) fn parse_source_bytes(source: &[u8], name: Option<&str>) -> ParsePayload {
    let name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_INLINE_NAME);
    let (source, map) = SourceMap::inline(source);
    let file = File::ephemeral(Cow::Owned(name.as_bytes().to_vec()), Cow::Owned(source));
    let source = AnalysisSource { file, map };

    parse_file_payload_record(&source)
}

pub(crate) fn source_bytes(
    source: Option<String>,
    source_hex: Option<String>,
) -> Result<Vec<u8>, String> {
    match (source, source_hex) {
        (_, Some(source_hex)) => decode_hex(&source_hex),
        (Some(source), None) => Ok(source.into_bytes()),
        (None, None) => Err("parse_source queries require source or source_hex".to_string()),
    }
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err("source_hex must contain an even number of characters".to_string());
    }

    hex.as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let high = decode_hex_digit(chunk[0])?;
            let low = decode_hex_digit(chunk[1])?;

            Ok((high << 4) | low)
        })
        .collect()
}

fn decode_hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("source_hex contains a non-hex character".to_string()),
    }
}

pub(crate) fn parse_file_path(path: &str) -> Result<ParsePayload, String> {
    let path = Path::new(path);
    let workspace = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let file = match File::read(&workspace, path, FileType::Host) {
        Ok(file) => file,
        Err(error) => return Err(format!("failed to read PHP source file: {error}")),
    };
    let source = AnalysisSource {
        map: SourceMap::file(&file),
        file,
    };

    Ok(parse_file_payload_record(&source))
}

pub(crate) fn parse_project_file_payload(relative: &str, contents: Vec<u8>) -> ParsePayload {
    let source = AnalysisSource {
        map: SourceMap::new(contents.clone(), 0, false),
        file: File::ephemeral(
            Cow::Owned(relative.as_bytes().to_vec()),
            Cow::Owned(contents),
        ),
    };

    parse_file_payload_record(&source)
}

fn parse_file_payload_record(source: &AnalysisSource) -> ParsePayload {
    let arena = Bump::new();
    let file = &source.file;
    let program = parse_file(&arena, file);
    let (tokens, lexer_errors) = collect_tokens(source);
    let errors = dedupe_errors(
        collect_errors(source, program)
            .into_iter()
            .chain(lexer_errors)
            .collect(),
    );
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

pub(crate) fn encode_json<T>(payload: &T) -> String
where
    T: Serialize,
{
    serde_json::to_string(payload)
        .unwrap_or_else(|error| error_json(format!("failed to encode code query result: {error}")))
}

impl SourceMap {
    fn inline(source: &[u8]) -> (Vec<u8>, Self) {
        let trimmed = trim_start_ascii(source);
        if trimmed.starts_with(b"<?php")
            || trimmed.starts_with(b"<?=")
            || trimmed.starts_with(b"<? ")
        {
            return (source.to_vec(), Self::new(source.to_vec(), 0, false));
        }

        let mut wrapped = INLINE_PREFIX.as_bytes().to_vec();
        wrapped.extend_from_slice(source);

        (
            wrapped,
            Self::new(source.to_vec(), INLINE_PREFIX.len() as u32, true),
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

fn trim_start_ascii(source: &[u8]) -> &[u8] {
    let first_non_space = source
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(source.len());

    &source[first_non_space..]
}

fn dedupe_errors(errors: Vec<ParseErrorRecord>) -> Vec<ParseErrorRecord> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for error in errors {
        let key = (
            error.message.clone(),
            error.span.start_offset,
            error.span.end_offset,
        );
        if seen.insert(key) {
            deduped.push(error);
        }
    }

    deduped
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

#[allow(clippy::too_many_arguments)]
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
    use std::fs;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use serde_json::Value;

    struct TestProject {
        root: PathBuf,
    }

    impl TestProject {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "dory-analysis-{name}-{}-{unique}",
                std::process::id()
            ));

            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("src")).expect("create test project");

            Self { root }
        }

        fn write_source(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create source parent directory");
            }

            fs::write(path, contents).expect("write PHP fixture");
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

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
    fn dispatches_hex_encoded_source_queries() {
        let json = dispatch_query_json(
            r#"{"op":"parse_source","source_hex":"636c6173732044656d6f207b7dff","name":"demo.php"}"#,
        );
        let payload: Value = serde_json::from_str(&json).expect("valid parse payload");

        assert_eq!(payload["ok"], true);
        assert_eq!(payload["declarations"][0]["name"], "Demo");
    }

    #[test]
    fn rejects_invalid_hex_encoded_source_queries() {
        let json = dispatch_query_json(r#"{"op":"parse_source","source_hex":"no"}"#);
        let payload: Value = serde_json::from_str(&json).expect("valid error payload");

        assert_eq!(payload["ok"], false);
        assert_eq!(
            payload["message"],
            "source_hex contains a non-hex character"
        );
    }

    #[test]
    fn indexes_and_queries_project_php_files() {
        let project = TestProject::new("query");
        let root = project.root.to_string_lossy().into_owned();
        project.write_source(
            "src/Example.php",
            "<?php namespace App; final class Example { public function run(): void {} }",
        );
        project.write_source("vendor/Ignored.php", "<?php class Ignored {}");

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
    }

    #[test]
    fn refreshes_project_index_when_file_contents_change() {
        let project = TestProject::new("refresh");
        let root = project.root.to_string_lossy().into_owned();
        project.write_source("src/Example.php", "<?php class Alpha {}");

        let alpha = serde_json::json!({
            "op": "query_declarations",
            "root": root.clone(),
            "query": {"kind": "class", "name": "Alpha"}
        });
        let payload: Value =
            serde_json::from_str(&dispatch_query_json(&alpha.to_string())).expect("valid payload");
        assert_eq!(payload["declarations"].as_array().unwrap().len(), 1);

        project.write_source("src/Example.php", "<?php class Bravo {}");

        let stale = serde_json::json!({
            "op": "query_declarations",
            "root": root.clone(),
            "query": {"kind": "class", "name": "Alpha"}
        });
        let payload: Value =
            serde_json::from_str(&dispatch_query_json(&stale.to_string())).expect("valid payload");
        assert_eq!(payload["declarations"].as_array().unwrap().len(), 0);

        let bravo = serde_json::json!({
            "op": "query_declarations",
            "root": root,
            "query": {"kind": "class", "name": "Bravo"}
        });
        let payload: Value =
            serde_json::from_str(&dispatch_query_json(&bravo.to_string())).expect("valid payload");
        assert_eq!(payload["declarations"].as_array().unwrap().len(), 1);
    }
}
