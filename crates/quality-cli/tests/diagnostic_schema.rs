#![cfg(unix)]

use std::fs;
use std::process::Command;

#[test]
fn generic_diagnostics_use_normalized_severity_and_positive_source_positions() {
    let directory = tempfile::tempdir().unwrap();
    let step = serde_json::json!({
        "command": "sh",
        "args": ["-c", "printf 'src/a.cpp:0:0: style: formatting finding\n'; exit 1"],
        "parser": "generic"
    });
    fs::write(
        directory.path().join("quality.yml"),
        serde_json::json!({
            "version": 1,
            "tasks": {"fixture": step},
            "hooks": {"pre-push": {"steps": [step]}}
        })
        .to_string(),
    )
    .unwrap();
    for (arguments, collection) in [
        (vec!["check", "--format", "json"], "results"),
        (
            vec!["ci", "local", "--format", "json", "--no-history"],
            "steps",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_quality"))
            .arg("--root")
            .arg(directory.path())
            .args(arguments)
            .env_remove("QUALITY_LOCAL_CI")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let diagnostic = &report[collection][0]["diagnostics"][0];
        assert_eq!(diagnostic["severity"], "info", "{report}");
        assert_eq!(diagnostic["line"], serde_json::Value::Null, "{report}");
        assert_eq!(diagnostic["column"], serde_json::Value::Null, "{report}");
    }
}
