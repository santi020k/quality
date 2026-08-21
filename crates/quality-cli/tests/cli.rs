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

fn git(root: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn initialize_git(root: &std::path::Path) {
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.email", "quality@example.test"]);
    git(root, &["config", "user.name", "Quality Tests"]);
}

#[test]
fn init_detects_a_mixed_mobile_project() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    fs::write(temp.path().join("MainActivity.kt"), "class MainActivity\n").unwrap();
    fs::write(temp.path().join("AndroidManifest.xml"), "<manifest />\n").unwrap();

    let output = quality(temp.path(), &["init"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config = fs::read_to_string(temp.path().join("quality.yml")).unwrap();
    assert!(config.contains("swiftlint:"));
    assert!(config.contains("swiftformat:"));
    assert!(config.contains("android-lint:"));
    assert!(config.contains("detekt:"));
    assert!(config.contains("ktlint:"));
    assert!(config.contains("baseline: .quality-baseline.json"));
    assert!(!config.contains("command: null"));
}

#[test]
fn configuration_typos_are_rejected_with_a_suggestion() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swfitlint:\n    enabled: true\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown tool `swfitlint`"));
    assert!(stderr.contains("Did you mean `swiftlint`?"));
}

#[test]
fn custom_tools_cannot_shadow_builtins() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools: {}\ncustom:\n  swiftlint:\n    command: custom-lint\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("conflicts with a built-in"));
}

#[test]
fn completions_are_generated_without_project_discovery() {
    let missing_root = "/definitely/not/a/quality/project";
    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(missing_root)
        .args(["completions", "fish"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("complete -c quality"));
}

#[test]
fn doctor_explains_a_missing_required_tool() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    required: true\n    command: definitely-not-a-real-tool\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SwiftLint"));
    assert!(stdout.contains("missing"));
    assert!(stdout.contains("brew install swiftlint"));
}

#[cfg(unix)]
#[test]
fn doctor_resolves_relative_commands_from_the_project_root() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("tools/swiftlint");
    fs::create_dir_all(fake.parent().unwrap()).unwrap();
    fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: ./tools/swiftlint\n  swiftformat:\n    enabled: false\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["doctor"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("✓ SwiftLint"));
}

#[test]
fn missing_required_tool_is_included_in_sarif() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: definitely-not-a-real-tool\n  swiftformat:\n    enabled: false\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["check", "--format", "sarif"]);
    assert_eq!(output.status.code(), Some(1));
    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        sarif["runs"][0]["results"][0]["ruleId"],
        "tool-not-installed"
    );
}

#[cfg(unix)]
#[test]
fn check_normalizes_a_tool_failure_to_json_and_sarif() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("fake-swiftlint");
    fs::write(
        &fake,
        "#!/bin/sh\necho 'App.swift:4:2: warning: Example problem (example_rule)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        format!(
            "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: {}\n  swiftformat:\n    enabled: false\n",
            fake.display()
        ),
    )
    .unwrap();

    let json_output = quality(temp.path(), &["check", "--format", "json"]);
    assert_eq!(json_output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(report["results"][0]["diagnostics"][0]["line"], 4);
    assert_eq!(
        report["results"][0]["diagnostics"][0]["rule"],
        "example_rule"
    );

    let sarif_output = quality(temp.path(), &["check", "--format", "sarif"]);
    assert_eq!(sarif_output.status.code(), Some(1));
    let sarif: serde_json::Value = serde_json::from_slice(&sarif_output.stdout).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "example_rule");
}

#[cfg(unix)]
#[test]
fn github_output_annotates_findings_and_writes_a_report() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("fake-swiftlint");
    fs::write(
        &fake,
        "#!/bin/sh\necho 'App,One.swift:7:3: warning: Coverage is 100% (coverage_rule)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        format!(
            "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: {}\n  swiftformat:\n    enabled: false\n",
            fake.display()
        ),
    )
    .unwrap();

    let output = quality(
        temp.path(),
        &[
            "check",
            "--format",
            "github",
            "--report",
            "reports/quality.sarif",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("::warning file=App%2COne.swift,line=7,col=3"));
    assert!(stdout.contains("Coverage is 100%25"));

    let report_path = temp.path().join("reports/quality.sarif");
    let sarif: serde_json::Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
    assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "coverage_rule");
    assert!(String::from_utf8_lossy(&output.stderr).contains("Wrote SARIF report"));
}

#[cfg(unix)]
#[test]
fn changed_mode_uses_swiftlints_supported_file_environment() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("fake-swiftlint");
    fs::write(
        &fake,
        "#!/bin/sh\nprintf '%s:%s:%s' \"$SCRIPT_INPUT_FILE_COUNT\" \"$SCRIPT_INPUT_FILE_0\" \"$*\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        format!(
            "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: {}\n  swiftformat:\n    enabled: false\n",
            fake.display()
        ),
    )
    .unwrap();
    git(temp.path(), &["add", "App.swift", "quality.yml"]);
    git(temp.path(), &["commit", "--quiet", "-m", "initial"]);
    fs::write(temp.path().join("App.swift"), "struct ChangedApp {}\n").unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["scope"]["mode"], "changed");
    assert_eq!(report["scope"]["files"], 2); // App.swift and the untracked fake executable.
    let tool_output = report["results"][0]["output"].as_str().unwrap();
    assert!(tool_output.starts_with("1:"));
    assert!(tool_output.contains("App.swift"));
    assert!(tool_output.contains("--use-script-input-files"));
}

#[cfg(unix)]
#[test]
fn changed_mode_passes_only_relevant_files_to_eslint() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("app.ts"), "const value = 1;\n").unwrap();
    fs::write(temp.path().join("notes.md"), "initial\n").unwrap();
    let tools_dir = temp.path().join("node_modules/.bin");
    fs::create_dir_all(&tools_dir).unwrap();
    let fake = tools_dir.join("eslint");
    fs::write(&fake, "#!/bin/sh\nprintf '%s' \"$*\"\n").unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  eslint:\n    enabled: true\n  prettier:\n    enabled: false\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "initial"]);
    fs::write(temp.path().join("app.ts"), "const value = 2;\n").unwrap();
    fs::write(temp.path().join("notes.md"), "changed\n").unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let command = report["results"][0]["command"].as_str().unwrap();
    assert!(command.contains("--format json app.ts"));
    assert!(!command.contains("notes.md"));
    assert!(!command.contains(" . "));
}

#[test]
fn changed_mode_requires_a_git_repository() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let output = quality(temp.path(), &["check", "--changed"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires Git"));
}

#[cfg(unix)]
#[test]
fn baseline_hides_existing_findings_but_not_new_ones() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("fake-swiftlint");
    fs::write(
        &fake,
        "#!/bin/sh\necho 'App.swift:4:2: warning: Existing problem (example_rule)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        format!(
            "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: {}\n  swiftformat:\n    enabled: false\n",
            fake.display()
        ),
    )
    .unwrap();

    let created = quality(temp.path(), &["baseline", "create"]);
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(temp.path().join(".quality-baseline.json").exists());

    let existing = quality(temp.path(), &["check"]);
    assert!(existing.status.success());
    assert!(String::from_utf8_lossy(&existing.stdout).contains("1 existing findings hidden"));

    fs::write(
        &fake,
        "#!/bin/sh\necho 'App.swift:40:2: warning: New problem (example_rule)'\nexit 1\n",
    )
    .unwrap();
    let new_finding = quality(temp.path(), &["check"]);
    assert_eq!(new_finding.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&new_finding.stdout).contains("New problem"));
}

#[test]
fn baseline_refuses_to_hide_a_missing_required_tool() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    command: definitely-not-a-real-tool\n  swiftformat:\n    enabled: false\n",
    )
    .unwrap();

    let output = quality(temp.path(), &["baseline", "create"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot create a safe baseline"));
    assert!(!temp.path().join(".quality-baseline.json").exists());
}

#[cfg(unix)]
#[test]
fn external_adapter_supports_detection_changed_files_and_normalization() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("widget.acme"), "initial\n").unwrap();
    let fake = temp.path().join("acme-lint");
    fs::write(
        &fake,
        "#!/bin/sh\necho 'widget.acme:8:2: warning: Company rule failed (acme-rule)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools: {}\ncustom:\n  acme-lint:\n    name: ACME Lint\n    command: ./acme-lint\n    extensions: [acme]\n    check_args: [scan]\n    parser: generic\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "initial"]);
    fs::write(temp.path().join("widget.acme"), "changed\n").unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["results"][0]["tool"], "acme-lint");
    assert_eq!(report["results"][0]["name"], "ACME Lint");
    assert!(
        report["results"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("scan widget.acme")
    );
    assert_eq!(report["results"][0]["diagnostics"][0]["rule"], "acme-rule");

    let doctor = quality(temp.path(), &["doctor"]);
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("ACME Lint"));

    let format_check = quality(temp.path(), &["format", "--check", "--format", "json"]);
    assert!(format_check.status.success());
    let format_report: serde_json::Value = serde_json::from_slice(&format_check.stdout).unwrap();
    assert_eq!(format_report["results"].as_array().unwrap().len(), 0);
}

#[test]
fn ci_generates_a_workflow_without_overwriting_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let install = "cargo install --git https://github.com/acme/quality --tag v0.1.0 --locked";
    let first = quality(temp.path(), &["ci", "github", "--install", install]);
    assert!(first.status.success());
    let workflow = temp.path().join(".github/workflows/quality.yml");
    assert!(workflow.exists());
    let workflow_text = fs::read_to_string(&workflow).unwrap();
    assert!(workflow_text.contains("quality check --format github --report quality.sarif"));
    assert!(workflow_text.contains("upload-sarif"));
    assert!(workflow_text.contains(install));
    assert!(!workflow_text.contains("__QUALITY_INSTALL_COMMAND__"));

    let second = quality(temp.path(), &["ci", "github", "--install", install]);
    assert_eq!(second.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&second.stderr).contains("--force"));
}

#[test]
fn ci_rejects_multiline_install_commands() {
    let temp = tempfile::tempdir().unwrap();
    let output = quality(
        temp.path(),
        &["ci", "github", "--install", "first line\nsecond line"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("one non-empty command line"));
    assert!(!temp.path().join(".github/workflows/quality.yml").exists());
}

#[cfg(unix)]
#[test]
fn first_run_init_doctor_and_check_work_without_global_tools() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("package.json"), "{}\n").unwrap();
    let bin = temp.path().join("node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("eslint"), "#!/bin/sh\necho '[]'\n").unwrap();
    fs::write(bin.join("prettier"), "#!/bin/sh\nexit 0\n").unwrap();
    for executable in [bin.join("eslint"), bin.join("prettier")] {
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).unwrap();
    }

    let initialized = quality(temp.path(), &["init"]);
    assert!(initialized.status.success());
    let doctor = quality(temp.path(), &["doctor"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stdout)
    );
    let checked = quality(temp.path(), &["check"]);
    assert!(
        checked.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("Quality checks passed"));
}
