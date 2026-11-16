use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn plan_for(path: &str) -> Value {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.name", "Impact tests"]);
    git(root, &["config", "user.email", "impact@example.invalid"]);
    fs::write(
        root.join("quality-impact.json"),
        include_str!("../../../quality-impact.json"),
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "Policy"]);
    let base = git(root, &["rev-parse", "HEAD"]);
    let changed = root.join(path);
    fs::create_dir_all(changed.parent().unwrap()).unwrap();
    fs::write(changed, "Changed fixture\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "Change"]);
    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(root)
        .args(["impact", "plan", "--base", &base, "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn selected(plan: &Value, id: &str) -> bool {
    plan["selected"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == id)
}

#[test]
fn action_scope_retains_analyzers_and_bundle_freshness() {
    let plan = plan_for("packages/action/src/main.ts");
    assert_eq!(plan["mode"], "affected");
    for id in [
        "action",
        "action-dist",
        "analyzers",
        "security",
        "release-contracts",
    ] {
        assert!(selected(&plan, id), "missing {id}");
    }
    assert!(!selected(&plan, "site-browser"));
    assert!(!selected(&plan, "rust"));
}

#[test]
fn rust_scope_retains_real_playground_validation() {
    let plan = plan_for("crates/quality-cli/src/tools.rs");
    assert_eq!(plan["mode"], "affected");
    assert!(selected(&plan, "rust"));
    assert!(selected(&plan, "playground"));
    assert!(selected(&plan, "analyzers"));
}

#[test]
fn every_shared_schema_and_planner_change_selects_complete_gate() {
    for path in [
        "apps/site/public/quality-doctor.schema.json",
        "apps/site/public/quality-ci-plan.schema.json",
        "apps/site/public/quality-local-ci.schema.json",
        "apps/site/public/quality-repositories.schema.json",
        "crates/quality-cli/src/impact.rs",
    ] {
        let plan = plan_for(path);
        assert_eq!(plan["mode"], "full", "{path}");
        assert!(selected(&plan, "rust"));
        assert!(selected(&plan, "site-browser"));
        assert!(selected(&plan, "action-dist"));
        assert!(selected(&plan, "playground"));
    }
}
