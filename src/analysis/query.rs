use super::records::{DeclarationRecord, TokenRecord};
use super::request::{DeclarationQuery, TokenQuery};

impl DeclarationQuery {
    pub(crate) fn matches(&self, declaration: &DeclarationRecord) -> bool {
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
    pub(crate) fn matches(&self, token: &TokenRecord) -> bool {
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
