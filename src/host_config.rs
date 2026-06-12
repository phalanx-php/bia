//! The phalanx.toml host layer (HB-01). Scope-locked to bia/SAPI/Swoole/
//! pre-PHP concerns: parse errors render before PHP exists, and app config
//! never enters TOML. Sections not consumed by the host directly are typed
//! facts awaiting the HostFacts handoff.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "phalanx.toml";

#[derive(Debug, Default)]
pub struct LoadedConfig {
    pub config: HostConfig,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfigError {
    path: PathBuf,
    detail: String,
}

impl Display for HostConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid {} at {}\n{}",
            FILE_NAME,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for HostConfigError {}

/// Walks from `start_dir` upward; the nearest phalanx.toml wins. A missing
/// file is not an error - bia runs config-free outside Phalanx projects.
pub fn load(start_dir: &Path) -> Result<LoadedConfig, HostConfigError> {
    let Some(path) = discover(start_dir) else {
        return Ok(LoadedConfig::default());
    };

    let raw = std::fs::read_to_string(&path).map_err(|error| HostConfigError {
        path: path.clone(),
        detail: error.to_string(),
    })?;

    let config = toml::from_str(&raw).map_err(|error| HostConfigError {
        path: path.clone(),
        detail: error.to_string(),
    })?;

    Ok(LoadedConfig {
        config,
        path: Some(path),
    })
}

fn discover(start_dir: &Path) -> Option<PathBuf> {
    start_dir
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|candidate| candidate.is_file())
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct HostConfig {
    pub php: PhpSection,
    pub swoole: SwooleSection,
    pub serve: Option<ServeSection>,
    pub bia: BiaSection,
    pub env: EnvSection,
    pub dev: DevSection,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct PhpSection {
    pub memory_limit: Option<MemoryLimit>,
    pub ini: BTreeMap<String, IniValue>,
}

/// PHP shorthand byte size: digits with an optional K/M/G suffix, or -1 for
/// unlimited. Kept as the original string because php.ini consumes it as-is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct MemoryLimit(String);

impl MemoryLimit {
    pub fn as_ini_value(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MemoryLimit {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let digits = value.strip_suffix(['K', 'M', 'G']).unwrap_or(&value);

        if digits == "-1" || (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())) {
            return Ok(Self(value));
        }

        Err(format!(
            "\"{value}\" is not a memory limit (expected forms like \"512M\", \"1G\", or -1)"
        ))
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum IniValue {
    Bool(bool),
    Int(i64),
    Str(String),
}

impl IniValue {
    pub fn as_ini_value(&self) -> String {
        match self {
            Self::Bool(true) => "1".to_string(),
            Self::Bool(false) => "0".to_string(),
            Self::Int(value) => value.to_string(),
            Self::Str(value) => value.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct SwooleSection {
    pub event_workers: WorkerCount,
    pub task_workers: u32,
    pub hooks: Vec<SwooleHook>,
}

impl Default for SwooleSection {
    fn default() -> Self {
        Self {
            event_workers: WorkerCount::Auto,
            task_workers: 0,
            hooks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "WorkerCountRepr")]
pub enum WorkerCount {
    Auto,
    Fixed(u32),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WorkerCountRepr {
    Count(u32),
    Word(String),
}

impl TryFrom<WorkerCountRepr> for WorkerCount {
    type Error = String;

    fn try_from(value: WorkerCountRepr) -> Result<Self, Self::Error> {
        match value {
            WorkerCountRepr::Count(0) => Err("a worker count must be at least 1".to_string()),
            WorkerCountRepr::Count(count) => Ok(Self::Fixed(count)),
            WorkerCountRepr::Word(word) if word == "auto" => Ok(Self::Auto),
            WorkerCountRepr::Word(word) => Err(format!(
                "\"{word}\" is not a worker count (expected \"auto\" or an integer)"
            )),
        }
    }
}

/// The declarative coroutine hook list (the v1 SWOOLE_HOOK_FILE lesson):
/// every hook is named against the known flag set, so a typo is a typed
/// refusal listing the valid names instead of a silently missing hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SwooleHook {
    Tcp,
    Udp,
    Unix,
    File,
    Sleep,
    Tls,
    StreamFunction,
    BlockingFunction,
    Proc,
    Curl,
    NativeCurl,
    Sockets,
    Stdio,
    PdoPgsql,
    PdoOdbc,
    PdoOracle,
    PdoSqlite,
    All,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ServeSection {
    pub listen: Listen,
}

impl ServeSection {
    /// Pre-PHP listen preflight: the host proves the address is bindable
    /// (port free, permissions sufficient) and releases it - the real bind
    /// belongs to the Swoole server PHP constructs.
    pub fn preflight(&self) -> Result<(), String> {
        match TcpListener::bind(self.listen.0) {
            Ok(listener) => {
                drop(listener);

                Ok(())
            }
            Err(error) => Err(format!("[serve] cannot bind {}: {error}", self.listen.0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Listen(pub SocketAddr);

impl TryFrom<String> for Listen {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse().map(Self).map_err(|_| {
            format!("\"{value}\" is not a listen address (expected ip:port, e.g. \"0.0.0.0:8080\")")
        })
    }
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct BiaSection {
    pub timeout: Option<Timeout>,
    pub concurrency: Option<u32>,
    pub verbose: bool,
}

/// A duration with an explicit unit: "500ms", "30s", "5m", "1h".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Timeout(pub Duration);

impl TryFrom<String> for Timeout {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let split = value
            .bytes()
            .position(|b| !b.is_ascii_digit())
            .filter(|&at| at > 0)
            .ok_or_else(|| duration_error(&value))?;

        let (digits, unit) = value.split_at(split);
        let amount: u64 = digits.parse().map_err(|_| duration_error(&value))?;

        let duration = match unit {
            "ms" => Duration::from_millis(amount),
            "s" => Duration::from_secs(amount),
            "m" => Duration::from_secs(amount * 60),
            "h" => Duration::from_secs(amount * 3600),
            _ => return Err(duration_error(&value)),
        };

        Ok(Self(duration))
    }
}

fn duration_error(value: &str) -> String {
    format!(
        "\"{value}\" is not a duration (expected forms like \"500ms\", \"30s\", \"5m\", \"1h\")"
    )
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct EnvSection {
    pub duplicate_keys: KeyPolicy,
    pub unused_keys: KeyPolicy,
}

impl Default for EnvSection {
    fn default() -> Self {
        Self {
            duplicate_keys: KeyPolicy::Warn,
            unused_keys: KeyPolicy::Silent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyPolicy {
    Error,
    Warn,
    Silent,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct DevSection {
    pub watch: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<HostConfig, toml::de::Error> {
        toml::from_str(raw)
    }

    fn parse_error(raw: &str) -> String {
        parse(raw)
            .expect_err("expected a typed refusal")
            .to_string()
    }

    #[test]
    fn an_empty_document_yields_the_default_config() {
        assert_eq!(parse("").unwrap(), HostConfig::default());
    }

    #[test]
    fn the_canonical_example_parses_into_typed_sections() {
        let config = parse(
            r#"
            [php]
            memory-limit = "512M"
            ini = { "opcache.enable" = true }

            [swoole]
            event-workers = "auto"
            task-workers = 4
            hooks = ["tcp", "file"]

            [serve]
            listen = "0.0.0.0:8080"

            [bia]
            timeout = "30s"
            concurrency = 50

            [env]
            duplicate-keys = "error"
            unused-keys = "warn"

            [dev]
            watch = ["app/", "routes.php"]
            "#,
        )
        .unwrap();

        assert_eq!(
            config.php.memory_limit.as_ref().unwrap().as_ini_value(),
            "512M"
        );
        assert_eq!(config.php.ini["opcache.enable"].as_ini_value(), "1");
        assert_eq!(config.swoole.event_workers, WorkerCount::Auto);
        assert_eq!(config.swoole.task_workers, 4);
        assert_eq!(config.swoole.hooks, vec![SwooleHook::Tcp, SwooleHook::File]);
        assert_eq!(
            config.serve.unwrap().listen.0,
            "0.0.0.0:8080".parse().unwrap()
        );
        assert_eq!(config.bia.timeout, Some(Timeout(Duration::from_secs(30))));
        assert_eq!(config.bia.concurrency, Some(50));
        assert_eq!(config.env.duplicate_keys, KeyPolicy::Error);
        assert_eq!(config.env.unused_keys, KeyPolicy::Warn);
        assert_eq!(config.dev.watch, vec!["app/", "routes.php"]);
    }

    #[test]
    fn a_misshapen_worker_count_is_a_typed_refusal() {
        let message = parse_error("[swoole]\ntask-workers = \"four\"");

        assert!(message.contains("task-workers"), "got: {message}");
    }

    #[test]
    fn worker_counts_accept_auto_or_a_positive_integer_only() {
        assert_eq!(
            parse("[swoole]\nevent-workers = 4")
                .unwrap()
                .swoole
                .event_workers,
            WorkerCount::Fixed(4),
        );
        assert!(parse_error("[swoole]\nevent-workers = 0").contains("at least 1"));
        assert!(parse_error("[swoole]\nevent-workers = \"five\"").contains("expected \"auto\""));
    }

    #[test]
    fn an_unknown_key_is_a_typed_refusal_not_a_silent_skip() {
        let message = parse_error("[serve]\nlisten = \"0.0.0.0:8080\"\nlistne = \"oops\"");

        assert!(message.contains("listne"), "got: {message}");
    }

    #[test]
    fn an_unknown_section_is_a_typed_refusal() {
        let message = parse_error("[database]\nurl = \"surreal://\"");

        assert!(message.contains("database"), "got: {message}");
    }

    #[test]
    fn a_listen_address_must_be_ip_and_port() {
        let message = parse_error("[serve]\nlisten = \"localhost:8080\"");

        assert!(message.contains("not a listen address"), "got: {message}");
    }

    #[test]
    fn a_serve_section_requires_listen() {
        let message = parse_error("[serve]");

        assert!(message.contains("listen"), "got: {message}");
    }

    #[test]
    fn a_unitless_timeout_is_a_typed_refusal() {
        assert_eq!(
            parse("[bia]\ntimeout = \"5m\"").unwrap().bia.timeout,
            Some(Timeout(Duration::from_secs(300))),
        );
        assert!(parse_error("[bia]\ntimeout = \"30\"").contains("not a duration"));
    }

    #[test]
    fn a_misspelled_hook_lists_the_valid_names() {
        let message = parse_error("[swoole]\nhooks = [\"tcpip\"]");

        assert!(message.contains("unknown variant"), "got: {message}");
        assert!(message.contains("tcp"), "got: {message}");
    }

    #[test]
    fn a_malformed_memory_limit_is_a_typed_refusal() {
        let message = parse_error("[php]\nmemory-limit = \"lots\"");

        assert!(message.contains("not a memory limit"), "got: {message}");
    }

    #[test]
    fn env_policies_reject_unknown_words() {
        let message = parse_error("[env]\nduplicate-keys = \"ignore\"");

        assert!(message.contains("unknown variant"), "got: {message}");
    }

    #[test]
    fn discovery_walks_upward_to_the_nearest_file() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("app/deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.path().join(FILE_NAME), "[bia]\nverbose = true\n").unwrap();

        let loaded = load(&nested).unwrap();

        assert!(loaded.config.bia.verbose);
        assert_eq!(loaded.path.unwrap(), root.path().join(FILE_NAME));
    }

    #[test]
    fn a_missing_file_loads_the_default_config_with_no_path() {
        let root = tempfile::tempdir().unwrap();

        let loaded = load(root.path()).unwrap();

        assert_eq!(loaded.config, HostConfig::default());
        assert!(loaded.path.is_none());
    }

    #[test]
    fn a_parse_failure_names_the_file_and_the_offending_key() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join(FILE_NAME),
            "[swoole]\ntask-workers = \"four\"\n",
        )
        .unwrap();

        let message = load(root.path())
            .expect_err("expected a typed refusal")
            .to_string();

        assert!(message.contains(FILE_NAME), "got: {message}");
        assert!(message.contains("task-workers"), "got: {message}");
    }

    #[test]
    fn the_serve_preflight_reports_an_occupied_port() {
        let holder = TcpListener::bind("127.0.0.1:0").unwrap();
        let section = ServeSection {
            listen: Listen(holder.local_addr().unwrap()),
        };

        let message = section.preflight().expect_err("port is held");

        assert!(message.contains("cannot bind"), "got: {message}");

        drop(holder);
        assert!(section.preflight().is_ok());
    }
}
