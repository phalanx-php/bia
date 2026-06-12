use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::error::BiaError;
use crate::host_config::LoadedConfig;

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(250);
const CHILD_STOP_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(host: &LoadedConfig, cwd: &Path, args: &[String]) -> Result<ExitCode, BiaError> {
    if args.is_empty() {
        return Err(BiaError::new(
            "dev:watch requires a Bia command to reload, for example: bia dev:watch run app.php",
        ));
    }

    let root = config_root(host, cwd);
    let watch_paths = watch_paths(host, cwd)?;

    run_composer_dump_autoload(&root)?;
    let mut child = spawn_child(&root, args)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown)).ok();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown)).ok();

    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(tx)
        .map_err(|error| BiaError::from_error("failed to start dev watcher", error))?;

    for path in &watch_paths {
        watcher.watch(path, recursive_mode(path)).map_err(|error| {
            BiaError::from_error(format!("failed to watch {}", path.display()), error)
        })?;
    }

    eprintln!("bia: watching {} path(s)", watch_paths.len());

    loop {
        if shutdown.load(Ordering::Relaxed) {
            stop_child(&mut child);

            return Ok(ExitCode::SUCCESS);
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| BiaError::from_error("failed to poll child command", error))?
        {
            return Ok(exit_code(status));
        }

        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if event.paths.is_empty() {
                    continue;
                }

                drain_pending_events(&rx);

                if let Err(error) = run_composer_dump_autoload(&root) {
                    eprintln!("bia: {error}");
                    continue;
                }

                stop_child(&mut child);
                child = spawn_child(&root, args)?;
            }
            Ok(Err(error)) => {
                return Err(BiaError::from_error(
                    "dev watcher received a filesystem error",
                    error,
                ));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(BiaError::new("dev watcher event stream disconnected"));
            }
        }
    }
}

fn drain_pending_events(rx: &std::sync::mpsc::Receiver<Result<notify::Event, notify::Error>>) {
    let deadline = Instant::now() + DEBOUNCE_WINDOW;

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn config_root(host: &LoadedConfig, cwd: &Path) -> PathBuf {
    host.path
        .as_ref()
        .and_then(|path| path.parent())
        .unwrap_or(cwd)
        .to_path_buf()
}

fn watch_paths(host: &LoadedConfig, cwd: &Path) -> Result<Vec<PathBuf>, BiaError> {
    if host.config.dev.watch.is_empty() {
        return Err(BiaError::new(
            "dev:watch requires [dev].watch to list at least one file or directory",
        ));
    }

    let root = config_root(host, cwd);
    let mut paths = Vec::with_capacity(host.config.dev.watch.len());

    for raw in &host.config.dev.watch {
        let path = Path::new(raw);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };

        if !resolved.exists() {
            return Err(BiaError::new(format!(
                "[dev].watch path does not exist: {}",
                resolved.display()
            )));
        }

        paths.push(resolved);
    }

    Ok(paths)
}

fn recursive_mode(path: &Path) -> RecursiveMode {
    if path.is_dir() {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    }
}

fn run_composer_dump_autoload(root: &Path) -> Result<(), BiaError> {
    let status = Command::new("composer")
        .args(["dump-autoload", "--quiet"])
        .current_dir(root)
        .status()
        .map_err(|error| BiaError::from_error("failed to run composer dump-autoload", error))?;

    if status.success() {
        return Ok(());
    }

    Err(BiaError::new(format!(
        "composer dump-autoload failed with status {status}"
    )))
}

fn spawn_child(root: &Path, args: &[String]) -> Result<Child, BiaError> {
    let current_exe = std::env::current_exe()
        .map_err(|error| BiaError::from_error("failed to locate current bia executable", error))?;

    Command::new(current_exe)
        .args(args)
        .current_dir(root)
        .spawn()
        .map_err(|error| BiaError::from_error("failed to start child Bia command", error))
}

fn stop_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }

    terminate_child(child);

    let deadline = Instant::now() + CHILD_STOP_TIMEOUT;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn terminate_child(child: &Child) {
    // SAFETY: child.id() is the exact PID returned by std::process::Child for
    // the process this watcher spawned. No process-name or port matching.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
}

fn exit_code(status: ExitStatus) -> ExitCode {
    match status.code() {
        Some(code) if (0..=u8::MAX as i32).contains(&code) => ExitCode::from(code as u8),
        Some(_) | None => ExitCode::from(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_config;

    #[test]
    fn watch_paths_requires_configured_paths() {
        let root = tempfile::tempdir().unwrap();
        let host = host_config::LoadedConfig::default();

        let error = watch_paths(&host, root.path()).expect_err("expected missing watch refusal");

        assert!(error.to_string().contains("[dev].watch"), "got: {error}");
    }

    #[test]
    fn watch_paths_resolve_from_config_root() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.path().join("app")).unwrap();

        let host = host_config::LoadedConfig {
            config: toml::from_str("[dev]\nwatch = [\"app\"]\n").unwrap(),
            path: Some(root.path().join("phalanx.toml")),
        };

        let paths = watch_paths(&host, nested.as_path()).expect("watch paths");

        assert_eq!(paths, vec![root.path().join("app")]);
    }

    #[test]
    fn watch_paths_refuses_missing_entries() {
        let root = tempfile::tempdir().unwrap();
        let host = host_config::LoadedConfig {
            config: toml::from_str("[dev]\nwatch = [\"missing\"]\n").unwrap(),
            path: Some(root.path().join("phalanx.toml")),
        };

        let error = watch_paths(&host, root.path()).expect_err("expected missing path refusal");

        assert!(error.to_string().contains("missing"), "got: {error}");
    }
}
