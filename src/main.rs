mod cli;
mod embed;
mod hooks;

use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clap::Parser;
use ripht_php_sapi::{CliRequest, RiphtSapi, SapiConfig};
use tempfile::NamedTempFile;

fn main() -> ExitCode {
    let cli = cli::DoryCli::parse();

    let run_mode = resolve_run_mode(&cli);

    if let RunMode::FileNotFound(path) = &run_mode {
        eprintln!("dory: script not found: {path}");
        return ExitCode::from(1);
    }

    let runtime = match embed::EmbeddedRuntime::extract() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("dory: failed to extract runtime: {e}");
            return ExitCode::from(1);
        }
    };

    RiphtSapi::configure(SapiConfig::new().sapi_name("cli"))
        .expect("Failed to configure SAPI");

    let php = RiphtSapi::instance();
    php.set_ini("swoole.use_shortname", "Off")
        .expect("INI error");
    php.set_ini("opcache.enable_cli", "1")
        .expect("INI error");
    php.set_ini("memory_limit", "512M")
        .expect("INI error");

    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown)).ok();
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown)).ok();

    let runtime_dir = runtime
        .runtime_path()
        .to_string_lossy()
        .into_owned();

    let cwd = std::env::current_dir().expect("Failed to determine current directory");

    let exit_file = NamedTempFile::new().expect("Failed to create exit code file");
    let exit_path = exit_file.path().to_string_lossy().into_owned();

    let (_inline_file, args_json) = match &run_mode {
        RunMode::Inline(code) => {
            let wrapped = wrap_inline_code(code);
            let mut f = NamedTempFile::with_suffix(".php")
                .expect("Failed to create inline script file");
            f.write_all(wrapped.as_bytes()).expect("Failed to write inline script");
            f.flush().expect("Failed to flush inline script");
            let path = f.path().to_string_lossy().into_owned();
            let json = serde_json::to_string(&["run", &path])
                .expect("Failed to serialize args");
            (Some(f), json)
        }
        RunMode::File(path) => {
            let json = serde_json::to_string(&["run", path.as_str()])
                .expect("Failed to serialize args");
            (None, json)
        }
        RunMode::Passthrough => {
            let json = serde_json::to_string(&cli.args)
                .expect("Failed to serialize args");
            (None, json)
        }
        RunMode::FileNotFound(_) => unreachable!(),
    };

    let mut req = CliRequest::new()
        .with_working_dir(&cwd)
        .with_env("DORY_RUNTIME_DIR", &runtime_dir)
        .with_env("DORY_ARGV", &args_json)
        .with_env("DORY_EXIT_FILE", &exit_path)
        .with_env("DORY_EMBEDDED", "1");

    if cli.verbose {
        req = req.with_env("DORY_VERBOSE", "1");
    }

    let ctx = match req.build(runtime.bootstrap.path()) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("dory: {e}");
            return ExitCode::from(1);
        }
    };

    let hooks = hooks::DoryHooks::new(Arc::clone(&shutdown));

    match php.execute_with_hooks(ctx, hooks) {
        Ok(_result) => {
            let code = read_exit_code(&exit_path);
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code)
            }
        }
        Err(e) => {
            eprintln!("dory: {e}");
            ExitCode::from(1)
        }
    }
}

enum RunMode {
    Inline(String),
    File(String),
    FileNotFound(String),
    Passthrough,
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.contains('\\') || s.ends_with(".php")
}

fn resolve_run_mode(cli: &cli::DoryCli) -> RunMode {
    if let Some(arg) = &cli.code {
        let path = Path::new(arg);
        if path.exists() {
            let abs = std::fs::canonicalize(path)
                .unwrap_or_else(|_| path.to_path_buf());
            return RunMode::File(abs.to_string_lossy().into_owned());
        }
        if looks_like_path(arg) {
            return RunMode::FileNotFound(arg.clone());
        }
        return RunMode::Inline(arg.clone());
    }

    if cli.args.is_empty() && !std::io::stdin().is_terminal() {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_ok() {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                return RunMode::Inline(trimmed.to_string());
            }
        }
    }

    RunMode::Passthrough
}

fn wrap_inline_code(code: &str) -> String {
    let code = code.trim();
    let is_expression = !code.contains(';') && !code.contains('{');

    let body = if is_expression {
        format!("$__r = ({code});\nif ($__r !== null) {{ dory()->dump($__r); }}\nreturn 0;")
    } else {
        let stmts = if code.ends_with(';') || code.ends_with('}') || code.ends_with("?>") {
            code.to_string()
        } else {
            format!("{code};")
        };
        format!("{stmts}\nreturn 0;")
    };

    format!("<?php declare(strict_types=1);\n{body}\n")
}

fn read_exit_code(path: &str) -> u8 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0)
}
