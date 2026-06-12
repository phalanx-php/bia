use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::host_config::{EnvSection, KeyPolicy};

#[derive(Debug, Default, Serialize)]
pub struct EnvMap {
    sources: Vec<String>,
    values: BTreeMap<String, EnvValue>,
    issues: Vec<EnvIssue>,
}

impl EnvMap {
    pub fn load(root: &Path, policy: &EnvSection) -> Self {
        let mut map = Self::default();

        for file in [".env", ".env.local"] {
            let path = root.join(file);
            if path.is_file() {
                map.sources.push(file.to_string());
                map.merge_file(&path, file, policy);
            }
        }

        map.sources.push("process env".to_string());
        for (key, value) in env::vars() {
            map.values.insert(
                key,
                EnvValue {
                    value,
                    source: "process env".to_string(),
                    line: None,
                },
            );
        }

        map
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|value| value.value.as_str())
    }

    fn merge_file(&mut self, path: &Path, label: &str, policy: &EnvSection) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.issue(
                    EnvSeverity::Error,
                    "file-unreadable",
                    format!("{label} could not be read: {error}"),
                    None,
                    Some(label.to_string()),
                    None,
                );
                return;
            }
        };

        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.issue(
                EnvSeverity::Warn,
                "bom-prefix",
                "file starts with a UTF-8 BOM".to_string(),
                None,
                Some(label.to_string()),
                Some(1),
            );
        }

        let raw = String::from_utf8_lossy(&bytes);
        let mut seen = BTreeSet::new();

        for (index, line) in raw.lines().enumerate() {
            let line_no = (index + 1) as u32;

            if line.ends_with('\r') {
                self.issue(
                    EnvSeverity::Warn,
                    "crlf-line-ending",
                    "line uses CRLF line ending".to_string(),
                    None,
                    Some(label.to_string()),
                    Some(line_no),
                );
            }

            let content = line.trim_start();
            if content.trim_end().is_empty() || content.starts_with('#') {
                continue;
            }

            let Some((key, value)) = content.split_once('=') else {
                self.issue(
                    EnvSeverity::Error,
                    "malformed-line",
                    "line is not KEY=value".to_string(),
                    None,
                    Some(label.to_string()),
                    Some(line_no),
                );
                continue;
            };

            if !valid_key(key) {
                self.issue(
                    EnvSeverity::Error,
                    "malformed-key",
                    format!("{key} is not an env key"),
                    Some(key.to_string()),
                    Some(label.to_string()),
                    Some(line_no),
                );
                continue;
            }

            if !seen.insert(key.to_string())
                && let Some(severity) = policy.duplicate_keys.severity()
            {
                self.issue(
                    severity,
                    "duplicate-key",
                    format!("{key} is defined more than once in {label}"),
                    Some(key.to_string()),
                    Some(label.to_string()),
                    Some(line_no),
                );
            }

            self.papercuts(key, value, label, line_no);

            self.values.insert(
                key.to_string(),
                EnvValue {
                    value: strip_quotes(value.trim()).to_string(),
                    source: label.to_string(),
                    line: Some(line_no),
                },
            );
        }
    }

    fn papercuts(&mut self, key: &str, value: &str, source: &str, line: u32) {
        if value != value.trim_end() {
            self.issue(
                EnvSeverity::Warn,
                "trailing-whitespace",
                format!("{key} has trailing whitespace"),
                Some(key.to_string()),
                Some(source.to_string()),
                Some(line),
            );
        }

        let trimmed = value.trim();
        if trimmed.is_empty() {
            self.issue(
                EnvSeverity::Warn,
                "empty-value",
                format!("{key} is empty"),
                Some(key.to_string()),
                Some(source.to_string()),
                Some(line),
            );
        }

        if quoted(trimmed) {
            self.issue(
                EnvSeverity::Warn,
                "quoted-value",
                format!("{key} is quoted"),
                Some(key.to_string()),
                Some(source.to_string()),
                Some(line),
            );
        }

        if trimmed.contains(['“', '”', '‘', '’']) {
            self.issue(
                EnvSeverity::Warn,
                "smart-quotes",
                format!("{key} uses smart quotes"),
                Some(key.to_string()),
                Some(source.to_string()),
                Some(line),
            );
        }

        if matches!(trimmed, "true" | "false" | "TRUE" | "FALSE") {
            self.issue(
                EnvSeverity::Warn,
                "string-boolean",
                format!("{key} looks like a string boolean"),
                Some(key.to_string()),
                Some(source.to_string()),
                Some(line),
            );
        }
    }

    fn issue(
        &mut self,
        severity: EnvSeverity,
        code: &'static str,
        message: String,
        key: Option<String>,
        source: Option<String>,
        line: Option<u32>,
    ) {
        self.issues.push(EnvIssue {
            severity,
            code,
            message,
            key,
            source,
            line,
        });
    }
}

#[derive(Debug, Serialize)]
pub struct EnvValue {
    value: String,
    source: String,
    line: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct EnvIssue {
    severity: EnvSeverity,
    code: &'static str,
    message: String,
    key: Option<String>,
    source: Option<String>,
    line: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvSeverity {
    Warn,
    Error,
}

trait KeyPolicySeverity {
    fn severity(self) -> Option<EnvSeverity>;
}

impl KeyPolicySeverity for KeyPolicy {
    fn severity(self) -> Option<EnvSeverity> {
        match self {
            Self::Error => Some(EnvSeverity::Error),
            Self::Warn => Some(EnvSeverity::Warn),
            Self::Silent => None,
        }
    }
}

fn valid_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };

    (first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

fn quoted(value: &str) -> bool {
    (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
}

fn strip_quotes(value: &str) -> &str {
    if quoted(value) {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, file: &str, contents: &str) {
        fs::write(dir.join(file), contents).unwrap();
    }

    #[test]
    fn env_files_capture_values_with_source_provenance() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".env", "SESSION_KEY=base\n");
        write(
            dir.path(),
            ".env.local",
            "SESSION_KEY=local\nPOOL_SIZE=16\n",
        );

        let map = EnvMap::load(dir.path(), &EnvSection::default());
        let value = map.values.get("SESSION_KEY").unwrap();

        assert_eq!(value.value, "local");
        assert_eq!(value.source, ".env.local");
        assert_eq!(value.line, Some(1));
        assert!(map.sources.contains(&".env".to_string()));
        assert!(map.sources.contains(&".env.local".to_string()));
    }

    #[test]
    fn parser_records_papercuts_and_duplicate_policy() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".env", "FLAG=true\nFLAG=\"false\" \nBAD LINE\n");

        let section = EnvSection {
            duplicate_keys: KeyPolicy::Error,
            ..EnvSection::default()
        };

        let map = EnvMap::load(dir.path(), &section);
        let codes: Vec<_> = map.issues.iter().map(|issue| issue.code).collect();

        assert!(codes.contains(&"string-boolean"));
        assert!(codes.contains(&"duplicate-key"));
        assert!(codes.contains(&"trailing-whitespace"));
        assert!(codes.contains(&"malformed-line"));
        assert!(
            map.issues
                .iter()
                .any(|issue| matches!(issue.severity, EnvSeverity::Error)
                    && issue.code == "duplicate-key")
        );
    }

    #[test]
    fn silent_duplicate_policy_suppresses_duplicate_issue() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".env", "A=1\nA=2\n");

        let section = EnvSection {
            duplicate_keys: KeyPolicy::Silent,
            unused_keys: KeyPolicy::Silent,
        };

        let map = EnvMap::load(dir.path(), &section);

        assert!(!map.issues.iter().any(|issue| issue.code == "duplicate-key"));
    }
}
