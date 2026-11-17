#![cfg(unix)]

use std::fs;
use std::process::Command;

#[test]
fn nested_project_annotations_and_sarif_use_workspace_paths() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().canonicalize().unwrap();
    let alias_directory = tempfile::tempdir().unwrap();
    let alias = alias_directory.path().join("workspace-alias");
    std::os::unix::fs::symlink(&workspace, &alias).unwrap();
    let root = workspace.join("packages/app");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("quality.yml"), serde_json::json!({
        "version": 1,
        "tasks": {"fixture": {
            "command": "sh", "args": ["-c", "printf 'src/a #.ts:2:3: error: test finding\\n'; exit 1"],
            "parser": "generic"
        }}
    }).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(&root)
        .args(["check", "--format", "github", "--report", "report.sarif"])
        .env("GITHUB_WORKSPACE", &alias)
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("file=packages/app/src/a #.ts,line=2,col=3"),
        "{output:?}"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("report.sarif")).unwrap()).unwrap();
    assert_eq!(
        report["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "packages/app/src/a%20%23.ts"
    );
    assert_eq!(
        report["runs"][0]["invocations"][0]["executionSuccessful"],
        true
    );
}

#[test]
fn baseline_create_accepts_explicit_execution_limits() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("quality.yml"), serde_json::json!({
        "version": 1,
        "tasks": {"fixture": {
            "command": "sh", "args": ["-c", "printf 'src/a.ts:1:1: error: existing finding\\n'; exit 1"],
            "parser": "generic"
        }}
    }).to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(directory.path())
        .args([
            "baseline",
            "create",
            "--max-output-bytes",
            "2097152",
            "--jobs",
            "1",
            "--timeout-seconds",
            "10",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 findings (1 occurrences)"),
        "{output:?}"
    );
}
