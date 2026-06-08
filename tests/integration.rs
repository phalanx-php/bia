use std::process::Command;

fn bia() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bia"))
}

fn bia_with_stdin(input: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_bia"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn bia");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");

    child.wait_with_output().expect("Failed to wait on bia")
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_bare_help() {
    let output = bia().output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "exit code: {}", output.status);
    assert!(stdout.contains("Usage:"), "stdout: {stdout}");
    assert!(stdout.contains("doctor"), "stdout: {stdout}");
    assert!(
        !stderr.contains("Missing required argument: script"),
        "stderr: {stderr}"
    );
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_version() {
    let output = bia().arg("--version").output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit code: {}", output.status);
    assert!(stdout.contains("bia"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_doctor() {
    let output = bia().arg("doctor").output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(
        stdout.contains("[pass]"),
        "doctor should show passes. stdout: {stdout}"
    );
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_inline_expression() {
    let output = bia().args(["-r", "1 + 1"]).output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("2"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_inline_dump() {
    let output = bia()
        .args(["-r", r#"bia()->dump("hello")"#])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("hello"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_inline_multistatement() {
    let output = bia()
        .args(["-r", r#"$x = 42; bia()->println((string)$x)"#])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_pipe_stdin() {
    let output = bia_with_stdin("1 + 1");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("2"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_run_fixture() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/return-value.php"
    );
    let output = bia()
        .args(["run", fixture])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("42"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_run_inline() {
    let output = bia()
        .args(["run", "1 + 1"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("2"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_r_file() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/return-value.php"
    );
    let output = bia().args(["-r", fixture]).output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("42"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_bare_var_assignment() {
    let output = bia()
        .args(["-r", "a = 1; b = 2; bia()->dump(a + b)"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("3"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_bare_var_fn_keyword_preserved() {
    let output = bia()
        .args(["-r", "array_map(fn(n) => n * 2, [1,2,3])"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("2"), "stdout: {stdout}");
    assert!(stdout.contains("4"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_bare_var_string_untouched() {
    let output = bia()
        .args(["-r", r#"bia()->dump("hello x world")"#])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("hello x world"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_run_nonexistent() {
    let output = bia()
        .args(["run", "nonexistent.php"])
        .output()
        .expect("failed to run");
    assert!(!output.status.success(), "should fail for missing script");
}
