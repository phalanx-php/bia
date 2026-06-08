use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    let root = workspace_root();

    match cmd {
        "embed" => run_embed(&root),
        "build" => run_build(&root, false),
        "release" => run_build(&root, true),
        "test" => run_test(&root),
        "check" => run_check(&root),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    eprintln!(
        "\
Usage: cargo xtask <command>

Commands:
  embed    Rebuild embedded PHP runtime tar from monorepo sources
  build    embed + cargo build (dev)
  release  embed + cargo build --release
  test     embed + build + cargo test (integration)
  check    embed + cargo check"
    );
}

fn find_php() -> String {
    if let Ok(path) = std::process::Command::new("which").arg("php").output() {
        let s = String::from_utf8_lossy(&path.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }

    for candidate in [
        "/opt/homebrew/opt/php@8.4/bin/php",
        "/opt/homebrew/bin/php",
        "/usr/local/bin/php",
        "/usr/bin/php",
    ] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    "php".to_string()
}

fn run_embed(root: &PathBuf) -> ExitCode {
    eprintln!(":: Rebuilding embedded runtime tar...");

    let php = find_php();

    let status = Command::new(&php)
        .args(["-d", "phar.readonly=0"])
        .arg(root.join("scripts/build-embed.php"))
        .current_dir(root)
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("embed failed (exit {})", s.code().unwrap_or(-1));
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("failed to run php: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_build(root: &PathBuf, release: bool) -> ExitCode {
    if run_embed(root) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    eprintln!(
        ":: Building bia binary{}...",
        if release { " (release)" } else { "" }
    );

    let mut cmd = Command::new("cargo");

    cmd.arg("build")
        .arg("--package")
        .arg("bia")
        .current_dir(root);

    if release {
        cmd.arg("--release");
    }

    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("build failed (exit {})", s.code().unwrap_or(-1));
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("failed to run cargo: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_test(root: &PathBuf) -> ExitCode {
    if run_build(root, false) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    eprintln!(":: Running integration tests...");

    let status = Command::new("cargo")
        .args(["test", "--package", "bia", "--", "--ignored"])
        .current_dir(root)
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("tests failed (exit {})", s.code().unwrap_or(-1));
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("failed to run cargo test: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_check(root: &PathBuf) -> ExitCode {
    if run_embed_stability_check(root) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    eprintln!(":: Checking bia...");

    let status = Command::new("cargo")
        .args(["check", "--package", "bia"])
        .current_dir(root)
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("check failed (exit {})", s.code().unwrap_or(-1));
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("failed to run cargo check: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_embed_stability_check(root: &PathBuf) -> ExitCode {
    if run_embed(root) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    let first = match fs::read(root.join("embedded/bia-runtime.tar")) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("failed to read embedded runtime tar after first build: {error}");

            return ExitCode::from(1);
        }
    };

    if run_embed(root) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    let second = match fs::read(root.join("embedded/bia-runtime.tar")) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("failed to read embedded runtime tar after second build: {error}");

            return ExitCode::from(1);
        }
    };

    if first != second {
        eprintln!("embedded runtime tar is not deterministic across consecutive builds");

        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside workspace")
        .to_path_buf()
}
