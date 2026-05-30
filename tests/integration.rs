use std::process::Command;

fn dory() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dory"))
}

fn dory_with_stdin(input: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_dory"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn dory");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");

    child.wait_with_output().expect("Failed to wait on dory")
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_version() {
    let output = dory().arg("--version").output().expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit code: {}", output.status);
    assert!(stdout.contains("dory"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_doctor() {
    let output = dory().arg("doctor").output().expect("failed to run");
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
    let output = dory()
        .args(["-r", "1 + 1"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("2"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_inline_dump() {
    let output = dory()
        .args(["-r", r#"dory()->dump("hello")"#])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("hello"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_inline_multistatement() {
    let output = dory()
        .args(["-r", r#"$x = 42; dory()->println((string)$x)"#])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_pipe_stdin() {
    let output = dory_with_stdin("1 + 1");
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
    let output = dory()
        .args(["run", fixture])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "exit: {}", output.status);
    assert!(stdout.trim().contains("42"), "stdout: {stdout}");
}

#[test]
#[ignore = "requires static PHP runtime"]
fn test_run_nonexistent() {
    let output = dory()
        .args(["run", "nonexistent.php"])
        .output()
        .expect("failed to run");
    assert!(!output.status.success(), "should fail for missing script");
}
