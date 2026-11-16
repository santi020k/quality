//! Regression coverage for the runner's process/output handling.
//!
//! These tests spawn real `sh` subprocesses whose children inherit the
//! `quality` CLI's piped stdout/stderr and keep those pipes open after the
//! immediate child has been killed or has exited normally. Every test is
//! guarded by an outer `wait_timeout` watchdog so a regression (the runner
//! blocking forever on the leftover pipe) fails the test instead of hanging
//! the suite.

#![cfg(any(unix, windows))]

use std::fs;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use wait_timeout::ChildExt;

/// Generous enough for CI jitter, but far below the delay a regression would
/// reintroduce; exceeding it kills the child and fails the test instead of
/// hanging indefinitely.
const WATCHDOG: Duration = Duration::from_secs(20);

/// Serializes the tests in this file so their subprocesses (each spawning
/// `sh` plus a background `sleep`) don't contend with one another for CPU
/// time, which otherwise makes the wall-clock assertions below flaky.
static SERIAL: Mutex<()> = Mutex::new(());

fn quality_command(root: &std::path::Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quality"));
    command
        .arg("--root")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Runs `quality` under a watchdog so a hang fails the test instead of the
/// whole suite. Returns the elapsed wall time plus the captured output.
fn run_with_watchdog(mut command: Command) -> (Duration, std::process::Output) {
    let started = std::time::Instant::now();
    let child = command.spawn().expect("quality should spawn");
    finish_with_watchdog(child, started)
}

fn finish_with_watchdog(
    mut child: Child,
    started: std::time::Instant,
) -> (Duration, std::process::Output) {
    let status = match child
        .wait_timeout(WATCHDOG)
        .expect("waiting for quality should not error")
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "quality did not exit within the {WATCHDOG:?} test watchdog; this \
                 likely means the runner is blocking on an inherited pipe again"
            );
        }
    };
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout)
            .expect("could not capture quality stdout");
    }
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr)
            .expect("could not capture quality stderr");
    }
    (
        started.elapsed(),
        std::process::Output {
            status,
            stdout,
            stderr,
        },
    )
}

fn write_config(root: &std::path::Path, config: &str) {
    fs::write(root.join("quality.yml"), config).unwrap();
}

#[test]
#[cfg(unix)]
fn check_honors_the_configured_timeout_even_when_a_descendant_inherits_the_pipe() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_config(
        temp.path(),
        r#"version: 1
output: pretty
tasks:
  background-sleep:
    command: sh
    args: ["-c", "sleep 6 & wait"]
    timeout_seconds: 1
"#,
    );

    let command = quality_command(temp.path(), &["check", "--format", "json"]);
    let (elapsed, output) = run_with_watchdog(command);

    assert!(
        elapsed < Duration::from_secs(4),
        "check should return shortly after its 1-second timeout, not wait for the \
         6-second descendant to release the inherited pipe; took {elapsed:?}"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("expected JSON stdout: {error}\n{output:?}"));
    let result = &report["results"][0];
    assert_eq!(result["tool"], "background-sleep");
    assert_eq!(result["status"], "failed");
    assert_eq!(result["diagnostics"][0]["rule"], "tool-timeout");
    assert!(
        result.get("output_truncated").is_none(),
        "terminating the process group should close every inherited pipe: {result}"
    );
}

#[test]
#[cfg(unix)]
fn check_bounds_capture_when_a_descendant_keeps_the_pipe_open_after_a_normal_exit() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_config(
        temp.path(),
        r#"version: 1
output: pretty
tasks:
  orphaned-background-job:
    command: sh
    args: ["-c", "printf 'hello-from-the-task\n'; (sleep 8 &) ; exit 0"]
"#,
    );

    let command = quality_command(temp.path(), &["check", "--format", "json"]);
    let (elapsed, output) = run_with_watchdog(command);

    assert!(
        elapsed < Duration::from_secs(5),
        "check should return once the parent process exits, not wait for the \
         8-second orphaned descendant to close the inherited pipe; took {elapsed:?}"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("expected JSON stdout: {error}\n{output:?}"));
    let result = &report["results"][0];
    assert_eq!(result["tool"], "orphaned-background-job");
    assert_eq!(result["status"], "passed");
    assert!(
        result["output"]
            .as_str()
            .unwrap_or_default()
            .contains("hello-from-the-task"),
        "the task's own output should still be captured: {result}"
    );
    assert_eq!(result["output_truncated"], true);
}

#[test]
#[cfg(unix)]
fn baseline_create_refuses_a_truncated_failure_even_when_the_retained_output_parses_cleanly() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_config(
        temp.path(),
        r#"version: 1
output: pretty
tasks:
  orphaned-eslint-like-task:
    command: sh
    args:
      - "-c"
      - printf '[{"filePath":"app.ts","messages":[{"ruleId":"semi","severity":2,"message":"Missing semicolon.","line":1,"column":1}]}]'; (sleep 8 &) ; exit 1
    parser: eslint-json
"#,
    );

    let command = quality_command(temp.path(), &["baseline", "create"]);
    let (elapsed, output) = run_with_watchdog(command);

    assert!(
        elapsed < Duration::from_secs(5),
        "baseline create should return once the parent process exits, not wait for \
         the 8-second orphaned descendant; took {elapsed:?}"
    );
    assert!(
        !output.status.success(),
        "a baseline must not be created from a truncated capture, even though the \
         eslint-json parser can fully parse the retained prefix: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot create a safe baseline"),
        "expected the unsafe-baseline guard to reject this run: {stderr}"
    );
    assert!(
        stderr.contains("orphaned-eslint-like-task"),
        "the error should name the offending task: {stderr}"
    );
    assert!(
        !temp.path().join(".quality-baseline.json").exists(),
        "no baseline file should be written when the capture is unsafe"
    );
}

#[test]
fn timeout_stops_descendant_side_effects_and_keeps_output() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_process_fixture_config(temp.path(), 2);

    let mut command = quality_command(temp.path(), &["check", "--format", "json"]);
    command.env("QUALITY_PROCESS_TIMEOUT_TEST_ROOT", temp.path());
    let (elapsed, output) = run_with_watchdog(command);
    assert!(elapsed < Duration::from_secs(7), "took {elapsed:?}");
    assert!(!output.status.success(), "{output:?}");
    assert!(
        temp.path().join("descendant-started").exists(),
        "the descendant must have started before the timeout: {output:?}"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("expected JSON stdout: {error}\n{output:?}"));
    let result = &report["results"][0];
    assert_eq!(result["diagnostics"][0]["rule"], "tool-timeout");
    assert!(result.get("guidance").is_none(), "{result}");
    let captured = result["output"].as_str().unwrap_or_default();
    assert!(captured.contains("stdout-before-timeout"), "{result}");
    assert!(captured.contains("stderr-before-timeout"), "{result}");

    // The child writes after five seconds. Waiting beyond that deadline proves
    // the timeout stopped its side effect, rather than merely closing output.
    std::thread::sleep(Duration::from_secs(6));
    assert!(
        !temp.path().join("descendant-finished").exists(),
        "the timed-out analyzer left a descendant running"
    );
}

fn write_process_fixture_config(root: &std::path::Path, timeout_seconds: u64) {
    let config = serde_json::json!({
        "version": 1,
        "tasks": {
            "delayed-descendant": {
                "command": std::env::current_exe().unwrap(),
                "args": ["--exact", "timeout_fixture_parent", "--nocapture"],
                "timeout_seconds": timeout_seconds
            }
        }
    });
    write_config(root, &config.to_string());
}

#[test]
#[cfg(unix)]
fn interrupt_and_termination_signals_reach_timed_descendants() {
    use std::os::unix::process::ExitStatusExt;

    let _serial = SERIAL.lock().unwrap();
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_process_fixture_config(temp.path(), 30);
        let mut command = quality_command(temp.path(), &["check", "--format", "json"]);
        command.env("QUALITY_PROCESS_TIMEOUT_TEST_ROOT", temp.path());
        let started = std::time::Instant::now();
        let mut child = command.spawn().unwrap();
        while !temp.path().join("descendant-started").exists() {
            if child.try_wait().unwrap().is_some() || started.elapsed() > Duration::from_secs(4) {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                panic!("signal fixture did not start: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            Command::new("/bin/kill")
                .args([&format!("-{signal}"), "--", &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let (elapsed, output) = finish_with_watchdog(child, started);
        assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
        assert_eq!(output.status.signal(), Some(signal), "{output:?}");
        std::thread::sleep(Duration::from_secs(6));
        assert!(
            !temp.path().join("descendant-finished").exists(),
            "signal {signal} left a timed analyzer descendant running"
        );
    }
}

#[test]
#[cfg(unix)]
fn suspending_and_resuming_quality_also_controls_timed_descendants() {
    use std::os::unix::process::ExitStatusExt;

    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    write_process_fixture_config(temp.path(), 30);
    let heartbeat = temp.path().join("descendant-heartbeat");
    let mut command = quality_command(temp.path(), &["check", "--format", "json"]);
    command
        .env("QUALITY_PROCESS_TIMEOUT_TEST_ROOT", temp.path())
        .env("QUALITY_PROCESS_HEARTBEAT_TEST", "1");
    let started = std::time::Instant::now();
    let mut child = command.spawn().unwrap();
    while !heartbeat.exists() {
        if child.try_wait().unwrap().is_some() || started.elapsed() > Duration::from_secs(4) {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("heartbeat fixture did not start: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let signal_child = |signal: &str| {
        Command::new("/bin/kill")
            .args([signal, "--", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    };
    let stop_sent = signal_child("-TSTP");
    let pause_started = std::time::Instant::now();
    let stopped = loop {
        let output = Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &child.id().to_string()])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&output.stdout)
            .trim()
            .starts_with('T')
        {
            break true;
        }
        if pause_started.elapsed() > Duration::from_secs(3) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    std::thread::sleep(Duration::from_millis(100));
    let paused_heartbeat = fs::read(&heartbeat).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let stayed_paused = fs::read(&heartbeat).unwrap() == paused_heartbeat;

    // Resume and terminate before assertions so a failed assertion cannot leave
    // the test's subprocesses suspended indefinitely.
    let continue_sent = signal_child("-CONT");
    let resume_started = std::time::Instant::now();
    let resumed = loop {
        if fs::read(&heartbeat).unwrap() != paused_heartbeat {
            break true;
        }
        if resume_started.elapsed() > Duration::from_secs(3) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let termination_sent = signal_child("-TERM");
    let (_, output) = finish_with_watchdog(child, started);
    assert!(stop_sent && continue_sent && termination_sent);
    assert!(stopped, "Quality did not suspend");
    assert!(
        stayed_paused,
        "the descendant kept running while Quality was suspended"
    );
    assert!(resumed, "the descendant did not resume with Quality");
    assert_eq!(output.status.signal(), Some(signal_hook::consts::SIGTERM));
}

// Launch this test binary itself as the portable analyzer fixture. The two
// fixture tests do nothing during the normal suite, and need no shell or
// language runtime to exercise the Windows process-tree path in CI.
#[test]
fn timeout_fixture_parent() {
    let Some(root) = std::env::var_os("QUALITY_PROCESS_TIMEOUT_TEST_ROOT") else {
        return;
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "timeout_fixture_descendant", "--nocapture"])
        .env("QUALITY_PROCESS_TIMEOUT_TEST_ROOT", root)
        .spawn()
        .unwrap();
    let _ = child.wait();
}

#[test]
fn timeout_fixture_descendant() {
    let Some(root) = std::env::var_os("QUALITY_PROCESS_TIMEOUT_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    fs::write(root.join("descendant-started"), "started").unwrap();
    println!("stdout-before-timeout");
    eprintln!("stderr-before-timeout");
    if std::env::var_os("QUALITY_PROCESS_HEARTBEAT_TEST").is_some() {
        for heartbeat in 0..200 {
            fs::write(root.join("descendant-heartbeat"), heartbeat.to_string()).unwrap();
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    std::thread::sleep(Duration::from_secs(5));
    fs::write(root.join("descendant-finished"), "finished").unwrap();
}
