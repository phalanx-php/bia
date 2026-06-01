use std::borrow::Cow;
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

const INLINE_PREFIX: &str = "<?php\n";

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

#[derive(Debug, Serialize)]
struct SourceFileRecord {
    id: u64,
    name: String,
    wrapped: bool,
}

#[derive(Debug, Serialize)]
struct SpanRecord {
    start_offset: u32,
    end_offset: u32,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
}

#[derive(Debug, Serialize)]
struct ParseErrorRecord {
    message: String,
    span: SpanRecord,
}

#[derive(Debug, Serialize)]
struct TokenRecord {
    kind: String,
    text: String,
    span: SpanRecord,
}

#[derive(Debug, Serialize)]
struct DeclarationRecord {
    kind: &'static str,
    name: String,
    namespace: Option<String>,
    declaring_type: Option<String>,
    fqn: String,
    span: SpanRecord,
    name_span: SpanRecord,
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

pub fn parse_source_json(source: &str, name: Option<&str>) -> String {
    let name = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("dory://inline.php");
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

fn parse_file_payload(source: &AnalysisSource) -> String {
    let arena = Bump::new();
    let file = &source.file;
    let program = parse_file(&arena, file);
    let (tokens, lexer_errors) = collect_tokens(source);
    let errors = collect_errors(source, program)
        .into_iter()
        .chain(lexer_errors)
        .collect();
    let declarations = collect_declarations(source, program);

    let payload = ParsePayload {
        ok: true,
        file: SourceFileRecord {
            id: file.id.as_u64(),
            name: String::from_utf8_lossy(&file.name).into_owned(),
            wrapped: source.map.wrapped,
        },
        has_errors: program.has_errors(),
        errors,
        tokens,
        declarations,
    };

    serde_json::to_string(&payload)
        .unwrap_or_else(|error| error_json(format!("failed to encode parse result: {error}")))
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
}
