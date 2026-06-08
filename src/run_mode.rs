use std::io::{IsTerminal, Read};
use std::path::Path;

use crate::cli;
use crate::error::BiaError;

pub enum RunMode {
    Help,
    Inline(String),
    File(String),
    FileNotFound(String),
    Passthrough,
}

pub fn resolve(cli: &cli::BiaCli) -> Result<RunMode, BiaError> {
    if let Some(arg) = &cli.code {
        return Ok(resolve_code_arg(arg));
    }

    if !cli.args.is_empty() {
        return Ok(RunMode::Passthrough);
    }

    if !std::io::stdin().is_terminal() {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|error| BiaError::from_error("failed to read stdin", error))?;

        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            return Ok(RunMode::Inline(trimmed.to_string()));
        }
    }

    Ok(RunMode::Help)
}

fn resolve_code_arg(arg: &str) -> RunMode {
    let path = Path::new(arg);
    if path.exists() {
        let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        return RunMode::File(abs.to_string_lossy().into_owned());
    }

    if looks_like_path(arg) {
        return RunMode::FileNotFound(arg.to_string());
    }

    RunMode::Inline(arg.to_string())
}

fn looks_like_path(s: &str) -> bool {
    if s.contains("://") || s.contains('(') || s.contains(';') || s.contains(' ') {
        return false;
    }

    s.contains('/') || s.contains('\\') || s.ends_with(".php")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_plain_code_as_inline() {
        match resolve_code_arg("1 + 1") {
            RunMode::Inline(code) => assert_eq!(code, "1 + 1"),
            _ => panic!("expected inline mode"),
        }
    }

    #[test]
    fn treats_missing_php_filename_as_missing_file() {
        match resolve_code_arg("missing.php") {
            RunMode::FileNotFound(path) => assert_eq!(path, "missing.php"),
            _ => panic!("expected missing file mode"),
        }
    }

    #[test]
    fn treats_url_like_text_as_inline() {
        match resolve_code_arg("https://example.com") {
            RunMode::Inline(code) => assert_eq!(code, "https://example.com"),
            _ => panic!("expected inline mode"),
        }
    }

    #[test]
    fn treats_positional_args_as_passthrough() {
        let cli = cli::BiaCli {
            verbose: false,
            code: None,
            args: vec!["doctor".to_string()],
        };

        match resolve(&cli).expect("run mode") {
            RunMode::Passthrough => {}
            _ => panic!("expected passthrough mode"),
        }
    }

    #[test]
    fn treats_existing_code_arg_path_as_file() {
        let file = tempfile::NamedTempFile::with_suffix(".php").expect("temp file");

        match resolve_code_arg(file.path().to_str().expect("utf8 path")) {
            RunMode::File(path) => assert!(path.ends_with(".php")),
            _ => panic!("expected file mode"),
        }
    }
}
