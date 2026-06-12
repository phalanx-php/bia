mod analysis;
mod cli;
mod embed;
mod error;
mod exit_status;
mod hooks;
mod host_config;
mod host_facts;
mod inline;
#[cfg(target_os = "linux")]
mod linux_compat;
mod native_functions;
mod run_mode;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use clap::{CommandFactory, Parser};
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

    if let RunMode::Help = &run_mode {
        let mut command = cli::BiaCli::command();
        command
            .print_help()
            .map_err(|error| BiaError::from_error("failed to print help", error))?;
        println!();

        return Ok(ExitCode::SUCCESS);
    }

    let cwd = std::env::current_dir()
        .map_err(|error| BiaError::from_error("failed to determine current directory", error))?;

    // The phalanx.toml gate: a broken host config refuses here, before any
    // PHP exists. Serve runs also prove the listen address pre-PHP; the
    // real bind belongs to the Swoole server PHP constructs.
    let host = host_config::load(&cwd).map_err(|error| BiaError::new(error.to_string()))?;

    if (cli.verbose || host.config.bia.verbose)
        && let Some(path) = &host.path
    {
        eprintln!("bia: using {}", path.display());
    }

    if matches!(&run_mode, RunMode::Passthrough)
        && cli.args.first().is_some_and(|arg| arg == "serve")
        && let Some(serve) = &host.config.serve
    {
        serve.preflight().map_err(BiaError::new)?;
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

    let memory_limit = host
        .config
        .php
        .memory_limit
        .as_ref()
        .map_or("512M", |limit| limit.as_ini_value());

    php.set_ini("memory_limit", memory_limit)
        .map_err(|error| BiaError::from_error("failed to set memory_limit", error))?;

    for (key, value) in &host.config.php.ini {
        php.set_ini(key.as_str(), value.as_ini_value())
            .map_err(|error| {
                BiaError::from_error(format!("failed to set [php] ini {key}"), error)
            })?;
    }

    let shutdown = Arc::new(AtomicBool::new(false));

    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown)).ok();

    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown)).ok();

    let exit_file = NamedTempFile::new()
        .map_err(|error| BiaError::from_error("failed to create exit code file", error))?;

    let exit_path = exit_file.path().to_string_lossy().into_owned();

    let (_inline_file, argv) = match &run_mode {
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

            (Some(f), vec!["run".to_string(), path])
        }
        RunMode::File(path) => (None, vec!["run".to_string(), path.clone()]),
        RunMode::Passthrough => (None, cli.args.clone()),
        RunMode::Help => {
            return Ok(ExitCode::SUCCESS);
        }
        RunMode::FileNotFound(path) => {
            return Err(BiaError::new(format!("script not found: {path}")));
        }
    };

    let app_env = host_facts::AppEnv::detect().map_err(BiaError::new)?;
    let effective_verbose = cli.verbose || host.config.bia.verbose;

    let facts = host_facts::HostFacts::gather(
        &host.config,
        app_env,
        argv,
        &runtime.runtime_path(),
        &cwd,
        memory_limit,
        effective_verbose,
    );

    host_facts::publish(&facts);

    let req = CliRequest::new()
        .with_working_dir(&cwd)
        .with_env("BIA_EXIT_FILE", &exit_path);

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
