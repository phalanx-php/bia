use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum CodeQueryRequest {
    ParseSource {
        source: Option<String>,
        source_hex: Option<String>,
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
    QueryNodes {
        root: String,
        #[serde(default)]
        query: NodeQuery,
    },
    QueryReferences {
        root: String,
        #[serde(default)]
        query: ReferenceQuery,
    },
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DeclarationQuery {
    pub(crate) kind: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) fqn: Option<String>,
    pub(crate) file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct TokenQuery {
    pub(crate) kind: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct NodeQuery {
    pub(crate) kind: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) context: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ReferenceQuery {
    pub(crate) kind: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) receiver: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) context: Option<String>,
}
