mod analysis;
mod cli;
mod embed;
mod error;
mod exit_status;
mod hooks;
mod inline;
#[cfg(target_os = "linux")]
mod linux_compat;
mod native_functions;
mod run_mode;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use clap::Parser;
use error::BiaError;
use ripht_php_sapi::{CliRequest, RiphtSapi, SapiConfig};
use run_mode::RunMode;
use tempfile::NamedTempFile;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("bia: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, BiaError> {
    let cli = cli::BiaCli::parse();

    let run_mode = run_mode::resolve(&cli)?;

    if let RunMode::FileNotFound(path) = &run_mode {
        return Err(BiaError::new(format!("script not found: {path}")));
    }

    let runtime = embed::EmbeddedRuntime::extract()
        .map_err(|error| BiaError::from_error("failed to extract runtime", error))?;

    RiphtSapi::configure(
        SapiConfig::new()
            .sapi_name("cli")
            .native_functions(native_functions::entries()),
    )
    .map_err(|error| BiaError::from_error("failed to configure SAPI", error))?;

    let php = RiphtSapi::instance();

    php.set_ini("swoole.use_shortname", "Off")
        .map_err(|error| BiaError::from_error("failed to set swoole.use_shortname", error))?;

    php.set_ini("memory_limit", "512M")
        .map_err(|error| BiaError::from_error("failed to set memory_limit", error))?;

    let shutdown = Arc::new(AtomicBool::new(false));

    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown)).ok();

    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown)).ok();

    let runtime_dir = runtime.runtime_path().to_string_lossy().into_owned();

    let cwd = std::env::current_dir()
        .map_err(|error| BiaError::from_error("failed to determine current directory", error))?;

    let exit_file = NamedTempFile::new()
        .map_err(|error| BiaError::from_error("failed to create exit code file", error))?;

    let exit_path = exit_file.path().to_string_lossy().into_owned();

    let (_inline_file, args_json) = match &run_mode {
        RunMode::Inline(code) => {
            let wrapped = inline::wrap_inline_code(code);

            let mut f = NamedTempFile::with_suffix(".php").map_err(|error| {
                BiaError::from_error("failed to create inline script file", error)
            })?;

            f.write_all(wrapped.as_bytes())
                .map_err(|error| BiaError::from_error("failed to write inline script", error))?;

            f.flush()
                .map_err(|error| BiaError::from_error("failed to flush inline script", error))?;

            let path = f.path().to_string_lossy().into_owned();

            let json = serde_json::to_string(&["run", &path])
                .map_err(|error| BiaError::from_error("failed to serialize args", error))?;

            (Some(f), json)
        }
        RunMode::File(path) => {
            let json = serde_json::to_string(&["run", path.as_str()])
                .map_err(|error| BiaError::from_error("failed to serialize args", error))?;
            (None, json)
        }
        RunMode::Passthrough => {
            let json = serde_json::to_string(&cli.args)
                .map_err(|error| BiaError::from_error("failed to serialize args", error))?;
            (None, json)
        }
        RunMode::FileNotFound(path) => {
            return Err(BiaError::new(format!("script not found: {path}")));
        }
    };

    let mut req = CliRequest::new()
        .with_working_dir(&cwd)
        .with_env("BIA_RUNTIME_DIR", &runtime_dir)
        .with_env("BIA_ARGV", &args_json)
        .with_env("BIA_EXIT_FILE", &exit_path)
        .with_env("BIA_EMBEDDED", "true");

    if cli.verbose {
        req = req.with_env("BIA_VERBOSE", "1");
    }

    let ctx = match req.build(runtime.bootstrap.path()) {
        Ok(ctx) => ctx,
        Err(error) => return Err(BiaError::new(error.to_string())),
    };

    let hooks = hooks::BiaHooks::new(Arc::clone(&shutdown));

    match php.execute_with_hooks(ctx, hooks) {
        Ok(_result) => {
            let code = exit_status::read_exit_code(exit_file.path())?;
            if code == 0 {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(code))
            }
        }
        Err(error) => Err(BiaError::new(error.to_string())),
    }
}
