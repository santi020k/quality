use std::fs;
use std::process::{Command, Output};

fn quality(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("quality should execute")
}

#[test]
fn portable_task_execution_emits_a_versioned_report() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: json\ntasks:\n  git-version:\n    command: git\n    args: [--version]\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["check", "--require-checks"]);

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["summary"]["tools"], 1);
    assert_eq!(report["summary"]["passed"], 1);
    assert_eq!(report["results"][0]["tool"], "git-version");
    assert_eq!(report["results"][0]["status"], "passed");
}

#[test]
fn portable_doctor_output_is_versioned() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("quality.yml"), "version: 1\ntools: {}\n").unwrap();

    let output = quality(temp.path(), &["doctor", "--format", "json"]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert!(report["tools"].is_array());
}

#[test]
fn invalid_configuration_uses_operational_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("quality.yml"), "version: 2\ntools: {}\n").unwrap();

    let output = quality(temp.path(), &["check"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported quality.yml version 2"));
}

#[test]
fn local_ci_executes_a_portable_hook_and_emits_a_versioned_report() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(
        temporary.path().join("quality.yml"),
        "version: 1\nhooks:\n  pre-push:\n    steps:\n      - name: Inspect Git\n        command: git\n        args: [--version]\n",
    )
    .unwrap();

    let output = quality(
        temporary.path(),
        &["ci", "local", "--format", "json", "--no-history"],
    );

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["status"], "passed");
    assert_eq!(report["steps"][0]["status"], "passed");
    assert!(report["steps"][0]["duration_ms"].is_number());
}

#[test]
fn local_ci_history_failure_does_not_change_a_passing_gate() {
    let temporary = tempfile::tempdir().unwrap();
    let initialized = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(temporary.path())
        .output()
        .unwrap();
    assert!(initialized.status.success());
    fs::write(
        temporary.path().join(".git/quality"),
        "blocks directory creation",
    )
    .unwrap();
    fs::write(
        temporary.path().join("quality.yml"),
        "version: 1\nhooks:\n  pre-push:\n    steps:\n      - command: git\n        args: [--version]\n",
    )
    .unwrap();

    let output = quality(temporary.path(), &["ci", "local", "--format", "json"]);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not retain local CI history"));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "passed");
}
