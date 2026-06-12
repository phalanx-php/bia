use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn bia() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bia"))
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind free port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn http_get(port: u16, path: &str, headers: &[(&str, &str)]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to bia serve");
    let mut request =
        format!("GET {path} HTTP/1.1\r\nHost: internal.example\r\nConnection: close\r\n");

    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }

    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).expect("write request");

    let mut response = String::new();
    match stream.read_to_string(&mut response) {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            if response.is_empty() {
                response.push_str("CONNECTION_RESET");
            }
        }
        Err(error) => panic!("read response: {error}"),
    }

    response
}

fn wait_for_server(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);

    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }

    panic!("bia serve did not start on port {port}");
}

fn terminate(child: &mut Child) {
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status();
}

fn wait_or_kill(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);

    while Instant::now() < deadline {
        if child.try_wait().expect("poll child").is_some() {
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }

    let _ = child.kill();
}

fn wait_for_file_contains(path: &std::path::Path, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);

    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(path)
            && contents.contains(needle)
        {
            return contents;
        }

        thread::sleep(Duration::from_millis(100));
    }

    let contents = std::fs::read_to_string(path).unwrap_or_default();
    panic!(
        "{} did not contain {needle:?}; contents: {contents}",
        path.display()
    );
}

fn marker_pid(contents: &str) -> u32 {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .expect("marker pid")
        .parse()
        .expect("numeric marker pid")
}

fn pid_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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
fn test_php_exit_code_reaches_host() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/exit-42.php");
    let output = bia()
        .args(["run", fixture])
        .output()
        .expect("failed to run");

    assert_eq!(output.status.code(), Some(42));
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

#[test]
#[ignore = "requires static PHP runtime with Swoole"]
fn test_serve_trusts_configured_forwarded_headers() {
    let temp = tempfile::tempdir().unwrap();
    let port = free_port();

    std::fs::write(
        temp.path().join("phalanx.toml"),
        format!(
            r#"
            [serve]
            listen = "127.0.0.1:{port}"
            behind-proxy = "nginx"
            "#
        ),
    )
    .unwrap();

    let mut child = bia()
        .arg("serve")
        .current_dir(temp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bia serve");

    wait_for_server(port);

    let response = http_get(
        port,
        "/",
        &[
            ("X-Forwarded-For", "203.0.113.10, 127.0.0.1"),
            ("X-Forwarded-Host", "public.example"),
            ("X-Forwarded-Proto", "https"),
        ],
    );

    terminate(&mut child);
    wait_or_kill(&mut child);

    assert!(response.contains("HTTP/1.1 200 OK"), "response: {response}");
    assert!(
        response.contains(r#""scheme":"https""#),
        "response: {response}"
    );
    assert!(
        response.contains(r#""host":"public.example""#),
        "response: {response}"
    );
    assert!(
        response.contains(r#""client_ip":"203.0.113.10""#),
        "response: {response}"
    );
}

#[test]
#[ignore = "requires static PHP runtime with Swoole"]
fn test_serve_sigterm_drains_and_refuses_new_work() {
    let temp = tempfile::tempdir().unwrap();
    let port = free_port();

    std::fs::write(
        temp.path().join("phalanx.toml"),
        format!(
            r#"
            [serve]
            listen = "127.0.0.1:{port}"

            [swoole]
            event-workers = 2

            [bia]
            timeout = "5s"
            "#
        ),
    )
    .unwrap();

    std::fs::write(
        temp.path().join("app.php"),
        r#"<?php
use Phalanx\Bia\Runtime\Serve\TrustedRequest;

return static function (TrustedRequest $trusted, Swoole\Http\Request $request): string {
    if (($request->server['request_uri'] ?? '/') === '/slow') {
        usleep(500000);

        return "slow done\n";
    }

    return "fast\n";
};
"#,
    )
    .unwrap();

    let mut child = bia()
        .args(["serve", "app.php"])
        .current_dir(temp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bia serve");

    wait_for_server(port);

    let slow = thread::spawn(move || http_get(port, "/slow", &[]));

    thread::sleep(Duration::from_millis(100));
    terminate(&mut child);

    let refused = http_get(port, "/", &[]);
    let slow_response = slow.join().expect("slow request thread");

    wait_or_kill(&mut child);

    assert!(
        slow_response.contains("slow done"),
        "slow response: {slow_response}"
    );
    assert!(
        refused.contains("503 Service Unavailable")
            || refused.contains("draining")
            || refused.contains("CONNECTION_RESET"),
        "refused response: {refused}"
    );
}

#[test]
#[ignore = "requires static PHP runtime and composer"]
fn test_dev_watch_refreshes_classmap_and_reloads_child() {
    let temp = tempfile::tempdir().unwrap();
    let app_dir = temp.path().join("app");
    let nested_dir = temp.path().join("work/deep");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::create_dir_all(&nested_dir).unwrap();

    std::fs::write(
        temp.path().join("phalanx.toml"),
        r#"
        [dev]
        watch = ["app"]
        "#,
    )
    .unwrap();

    std::fs::write(
        temp.path().join("composer.json"),
        r#"{"autoload":{"classmap":["app/"]}}"#,
    )
    .unwrap();

    std::fs::write(
        app_dir.join("First.php"),
        "<?php\nnamespace App;\nfinal class First {}\n",
    )
    .unwrap();

    std::fs::write(
        temp.path().join("probe.php"),
        r#"<?php
require __DIR__ . '/vendor/autoload.php';

$state = class_exists('App\\Second') ? 'loaded' : 'missing';
file_put_contents(__DIR__ . '/probe-status.txt', "pid=" . getmypid() . "\nstate={$state}\n");

while (true) {
    usleep(100000);
}
"#,
    )
    .unwrap();

    Command::new("composer")
        .args(["dump-autoload", "--quiet"])
        .current_dir(temp.path())
        .status()
        .expect("initial composer dump-autoload");

    let marker = temp.path().join("probe-status.txt");
    let mut child = bia()
        .args(["dev:watch", "run", "probe.php"])
        .current_dir(nested_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bia dev:watch");

    let initial_marker = wait_for_file_contains(&marker, "missing");
    let initial_pid = marker_pid(&initial_marker);

    std::fs::write(
        app_dir.join("Second.php"),
        "<?php\nnamespace App;\nfinal class Second {}\n",
    )
    .unwrap();

    let reloaded_marker = wait_for_file_contains(&marker, "loaded");
    let reloaded_pid = marker_pid(&reloaded_marker);

    assert_ne!(
        initial_pid, reloaded_pid,
        "dev:watch should restart the child process"
    );
    assert!(
        !pid_is_alive(initial_pid),
        "dev:watch left the old child process alive: {initial_pid}"
    );

    terminate(&mut child);
    wait_or_kill(&mut child);
}
