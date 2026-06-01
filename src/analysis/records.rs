use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SourceFileRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) wrapped: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SpanRecord {
    pub(crate) start_offset: u32,
    pub(crate) end_offset: u32,
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ParseErrorRecord {
    pub(crate) message: String,
    pub(crate) span: SpanRecord,
}

impl ParseErrorRecord {
    pub(crate) fn with_file(mut self, file: &str) -> Self {
        self.message = format!("{file}: {}", self.message);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TokenRecord {
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) span: SpanRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
}

impl TokenRecord {
    pub(crate) fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DeclarationRecord {
    pub(crate) kind: &'static str,
    pub(crate) name: String,
    pub(crate) namespace: Option<String>,
    pub(crate) declaring_type: Option<String>,
    pub(crate) fqn: String,
    pub(crate) span: SpanRecord,
    pub(crate) name_span: SpanRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
}

impl DeclarationRecord {
    pub(crate) fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ParsePayload {
    pub(crate) ok: bool,
    pub(crate) file: SourceFileRecord,
    pub(crate) has_errors: bool,
    pub(crate) errors: Vec<ParseErrorRecord>,
    pub(crate) tokens: Vec<TokenRecord>,
    pub(crate) declarations: Vec<DeclarationRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ErrorPayload {
    pub(crate) ok: bool,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectIndexPayload {
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) files: Vec<SourceFileRecord>,
    pub(crate) file_count: usize,
    pub(crate) declaration_count: usize,
    pub(crate) token_count: usize,
    pub(crate) errors: Vec<ParseErrorRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeclarationQueryPayload {
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) declarations: Vec<DeclarationRecord>,
    pub(crate) errors: Vec<ParseErrorRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TokenQueryPayload {
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) tokens: Vec<TokenRecord>,
    pub(crate) errors: Vec<ParseErrorRecord>,
}
