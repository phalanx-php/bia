use super::records::{
    DeclarationQueryPayload, ErrorPayload, ParsePayload, ProjectIndexPayload, TokenQueryPayload,
};
use super::request::{CodeQueryRequest, DeclarationQuery, TokenQuery};

#[derive(Clone, Copy, Debug, Default)]
pub struct CodeQueryEngine;

impl CodeQueryEngine {
    pub fn new() -> Self {
        Self
    }

    pub(crate) fn parse_source(&self, source: &[u8], name: Option<&str>) -> ParsePayload {
        super::parse_source_bytes(source, name)
    }

    pub(crate) fn parse_file(&self, path: &str) -> Result<ParsePayload, String> {
        super::parse_file_path(path)
    }

    pub(crate) fn index_project(&self, root: &str) -> Result<ProjectIndexPayload, String> {
        let index = super::project_index(root)?;

        Ok(ProjectIndexPayload {
            ok: true,
            root: index.root,
            file_count: index.files.len(),
            declaration_count: index.declarations.len(),
            token_count: index.tokens.len(),
            files: index.files,
            errors: index.errors,
        })
    }

    pub(crate) fn query_declarations(
        &self,
        root: &str,
        query: &DeclarationQuery,
    ) -> Result<DeclarationQueryPayload, String> {
        let index = super::project_index(root)?;
        let declarations = index
            .declarations
            .into_iter()
            .filter(|declaration| query.matches(declaration))
            .collect();

        Ok(DeclarationQueryPayload {
            ok: true,
            root: index.root,
            declarations,
            errors: index.errors,
        })
    }

    pub(crate) fn query_tokens(
        &self,
        root: &str,
        query: &TokenQuery,
    ) -> Result<TokenQueryPayload, String> {
        let index = super::project_index(root)?;
        let tokens = index
            .tokens
            .into_iter()
            .filter(|token| query.matches(token))
            .collect();

        Ok(TokenQueryPayload {
            ok: true,
            root: index.root,
            tokens,
            errors: index.errors,
        })
    }

    pub fn dispatch_json(&self, request_json: &str) -> String {
        let request = match serde_json::from_str::<CodeQueryRequest>(request_json) {
            Ok(request) => request,
            Err(error) => return error_json(format!("invalid Dory code query request: {error}")),
        };

        match request {
            CodeQueryRequest::ParseSource {
                source,
                source_hex,
                name,
            } => match super::source_bytes(source, source_hex) {
                Ok(source) => super::encode_json(&self.parse_source(&source, name.as_deref())),
                Err(error) => error_json(error),
            },
            CodeQueryRequest::ParseFile { path } => match self.parse_file(&path) {
                Ok(payload) => super::encode_json(&payload),
                Err(error) => error_json(error),
            },
            CodeQueryRequest::IndexProject { root } => match self.index_project(&root) {
                Ok(payload) => super::encode_json(&payload),
                Err(error) => error_json(error),
            },
            CodeQueryRequest::QueryDeclarations { root, query } => {
                match self.query_declarations(&root, &query) {
                    Ok(payload) => super::encode_json(&payload),
                    Err(error) => error_json(error),
                }
            }
            CodeQueryRequest::QueryTokens { root, query } => match self.query_tokens(&root, &query)
            {
                Ok(payload) => super::encode_json(&payload),
                Err(error) => error_json(error),
            },
        }
    }
}

#[cfg(test)]
pub fn dispatch_query_json(request_json: &str) -> String {
    CodeQueryEngine::new().dispatch_json(request_json)
}

pub fn error_json(message: impl Into<String>) -> String {
    serde_json::to_string(&ErrorPayload {
        ok: false,
        message: message.into(),
    })
    .unwrap_or_else(|_| "{\"ok\":false,\"message\":\"failed to encode error\"}".to_string())
}
