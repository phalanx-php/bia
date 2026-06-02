use super::records::{CodeNodeRecord, DeclarationRecord, ReferenceRecord, TokenRecord};
use super::request::{DeclarationQuery, NodeQuery, ReferenceQuery, TokenQuery};

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

impl NodeQuery {
    pub(crate) fn matches(&self, node: &CodeNodeRecord) -> bool {
        self.kind.as_deref().is_none_or(|kind| node.kind == kind)
            && self.name.as_deref().is_none_or(|name| {
                node.name
                    .as_deref()
                    .is_some_and(|node_name| node_name == name)
            })
            && self.context.as_deref().is_none_or(|context| {
                node.context
                    .as_deref()
                    .is_some_and(|node_context| node_context == context)
            })
            && self.file.as_deref().is_none_or(|file| {
                node.file
                    .as_deref()
                    .is_some_and(|node_file| node_file == file)
            })
    }
}

impl ReferenceQuery {
    pub(crate) fn matches(&self, reference: &ReferenceRecord) -> bool {
        self.kind
            .as_deref()
            .is_none_or(|kind| reference.kind == kind)
            && self
                .name
                .as_deref()
                .is_none_or(|name| reference.name == name)
            && self.context.as_deref().is_none_or(|context| {
                reference
                    .context
                    .as_deref()
                    .is_some_and(|reference_context| reference_context == context)
            })
            && self.file.as_deref().is_none_or(|file| {
                reference
                    .file
                    .as_deref()
                    .is_some_and(|reference_file| reference_file == file)
            })
    }
}
