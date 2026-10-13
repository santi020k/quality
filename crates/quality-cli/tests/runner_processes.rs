//! Regression coverage for the runner's process/output handling.
//!
//! These tests spawn real `sh` subprocesses whose children inherit the
//! `quality` CLI's piped stdout/stderr and keep those pipes open after the
//! immediate child has been killed or has exited normally. Every test is
//! guarded by an outer `wait_timeout` watchdog so a regression (the runner
//! blocking forever on the leftover pipe) fails the test instead of hanging
//! the suite.

#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
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
    let mut child = command.spawn().expect("quality should spawn");
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
    assert_eq!(result["output_truncated"], true);
}

#[test]
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
