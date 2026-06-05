use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiaError {
    message: String,
}

impl BiaError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn from_error(context: impl Display, error: impl Display) -> Self {
        Self::new(format!("{context}: {error}"))
    }
}

impl Display for BiaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BiaError {}
