mod cli;
mod embed;
mod error;
mod exit_status;
mod hooks;
mod inline;
mod run_mode;

use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clap::Parser;
use error::DoryError;
use ripht_php_sapi::{CliRequest, RiphtSapi, SapiConfig};
use run_mode::RunMode;
use tempfile::NamedTempFile;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("dory: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, DoryError> {
    let cli = cli::DoryCli::parse();

    let run_mode = run_mode::resolve(&cli)?;

    if let RunMode::FileNotFound(path) = &run_mode {
        return Err(DoryError::new(format!("script not found: {path}")));
    }

    let runtime = embed::EmbeddedRuntime::extract()
        .map_err(|error| DoryError::from_error("failed to extract runtime", error))?;

    RiphtSapi::configure(SapiConfig::new().sapi_name("cli"))
        .map_err(|error| DoryError::from_error("failed to configure SAPI", error))?;

    let php = RiphtSapi::instance();
    php.set_ini("swoole.use_shortname", "Off")
        .map_err(|error| DoryError::from_error("failed to set swoole.use_shortname", error))?;
    php.set_ini("opcache.enable_cli", "1")
        .map_err(|error| DoryError::from_error("failed to set opcache.enable_cli", error))?;
    php.set_ini("memory_limit", "512M")
        .map_err(|error| DoryError::from_error("failed to set memory_limit", error))?;

    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown)).ok();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown)).ok();

    let runtime_dir = runtime.runtime_path().to_string_lossy().into_owned();

    let cwd = std::env::current_dir()
        .map_err(|error| DoryError::from_error("failed to determine current directory", error))?;

    let exit_file = NamedTempFile::new()
        .map_err(|error| DoryError::from_error("failed to create exit code file", error))?;
    let exit_path = exit_file.path().to_string_lossy().into_owned();

    let (_inline_file, args_json) = match &run_mode {
        RunMode::Inline(code) => {
            let wrapped = inline::wrap_inline_code(code);
            let mut f = NamedTempFile::with_suffix(".php").map_err(|error| {
                DoryError::from_error("failed to create inline script file", error)
            })?;
            f.write_all(wrapped.as_bytes())
                .map_err(|error| DoryError::from_error("failed to write inline script", error))?;
            f.flush()
                .map_err(|error| DoryError::from_error("failed to flush inline script", error))?;
            let path = f.path().to_string_lossy().into_owned();
            let json = serde_json::to_string(&["run", &path])
                .map_err(|error| DoryError::from_error("failed to serialize args", error))?;
            (Some(f), json)
        }
        RunMode::File(path) => {
            let json = serde_json::to_string(&["run", path.as_str()])
                .map_err(|error| DoryError::from_error("failed to serialize args", error))?;
            (None, json)
        }
        RunMode::Passthrough => {
            let json = serde_json::to_string(&cli.args)
                .map_err(|error| DoryError::from_error("failed to serialize args", error))?;
            (None, json)
        }
        RunMode::FileNotFound(path) => {
            return Err(DoryError::new(format!("script not found: {path}")))
        }
    };

    let mut req = CliRequest::new()
        .with_working_dir(&cwd)
        .with_env("DORY_RUNTIME_DIR", &runtime_dir)
        .with_env("DORY_ARGV", &args_json)
        .with_env("DORY_EXIT_FILE", &exit_path)
        .with_env("DORY_EMBEDDED", "true");

    if cli.verbose {
        req = req.with_env("DORY_VERBOSE", "1");
    }

    let ctx = match req.build(runtime.bootstrap.path()) {
        Ok(ctx) => ctx,
        Err(error) => return Err(DoryError::new(error.to_string())),
    };

    let hooks = hooks::DoryHooks::new(Arc::clone(&shutdown));

    match php.execute_with_hooks(ctx, hooks) {
        Ok(_result) => {
            let code = exit_status::read_exit_code(exit_file.path())?;
            if code == 0 {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(code))
            }
        }
        Err(error) => Err(DoryError::new(error.to_string())),
    }
}
