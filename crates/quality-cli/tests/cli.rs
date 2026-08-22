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

fn quality_with_path(root: &std::path::Path, args: &[&str], path: &std::path::Path) -> Output {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let joined = std::env::join_paths(
        std::iter::once(path.to_path_buf()).chain(std::env::split_paths(&inherited)),
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(root)
        .args(args)
        .env("PATH", joined)
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
    fs::write(temp.path().join(".swiftlint.yml"), "\n").unwrap();
    fs::write(temp.path().join(".swiftformat"), "\n").unwrap();
    fs::write(temp.path().join("MainActivity.kt"), "class MainActivity\n").unwrap();
    fs::write(temp.path().join("AndroidManifest.xml"), "<manifest />\n").unwrap();
    fs::write(temp.path().join("detekt.yml"), "build: {}\n").unwrap();
    fs::write(temp.path().join(".editorconfig"), "root = true\n").unwrap();

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
    assert!(config.contains("cspell:ignore"));
    assert!(config.contains("clippy"));
    assert!(!config.contains("command: null"));
}

#[test]
fn init_preserves_the_canonical_repository_check() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{
            "packageManager":"pnpm@11.0.0",
            "scripts":{"verify":"pnpm run lint && pnpm run typecheck"},
            "devDependencies":{"eslint":"9.0.0"}
        }"#,
    )
    .unwrap();

    let output = quality(temp.path(), &["init"]);

    assert!(output.status.success());
    let config = fs::read_to_string(temp.path().join("quality.yml")).unwrap();
    assert!(config.contains("eslint:"));
    assert!(config.contains("check: false"));
    assert!(config.contains("repository-check:"));
    assert!(config.contains("name: Repository check (verify)"));
    assert!(config.contains("command: pnpm"));
    assert!(config.contains("- verify"));
}

#[test]
fn init_selects_fast_and_full_repository_gates_explicitly() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"verify:fast":"eslint .","verify:full":"pnpm test && pnpm build"}}"#,
    )
    .unwrap();

    let fast = quality(temp.path(), &["init", "--dry-run", "--gate", "fast"]);
    assert!(fast.status.success());
    let fast = String::from_utf8_lossy(&fast.stdout);
    assert!(fast.contains("Repository check (verify:fast)"));
    assert!(!fast.contains("Repository check (verify:full)"));

    let full = quality(temp.path(), &["init", "--dry-run", "--gate", "full"]);
    assert!(full.status.success());
    let full = String::from_utf8_lossy(&full.stdout);
    assert!(full.contains("Repository check (verify:full)"));
}

#[test]
fn doctor_distinguishes_disabled_checks_from_disabled_tools() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"scripts":{"verify":"printf ok"},"devDependencies":{"prettier":"3.0.0"}}"#,
    )
    .unwrap();
    let output = quality(temp.path(), &["init"]);
    assert!(output.status.success());

    let doctor = quality(temp.path(), &["doctor"]);
    assert!(doctor.status.success());
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains("check disabled; format/fix available")
    );
}

#[test]
fn init_does_not_import_a_recursive_quality_script() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{
            "scripts":{
                "validate":"pnpm run check",
                "check":"cargo run --package quality-cli -- check"
            },
            "devDependencies":{"eslint":"9.0.0"}
        }"#,
    )
    .unwrap();

    let output = quality(temp.path(), &["init"]);

    assert!(output.status.success());
    let config = fs::read_to_string(temp.path().join("quality.yml")).unwrap();
    assert!(!config.contains("repository-check:"));
    assert!(!config.contains("check: false"));
}

#[test]
fn init_imports_typecheck_when_there_is_no_composite_gate() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{
            "packageManager":"yarn@4.0.0",
            "scripts":{"type-check":"turbo type-check"},
            "devDependencies":{"eslint":"9.0.0"}
        }"#,
    )
    .unwrap();

    let output = quality(temp.path(), &["init"]);

    assert!(output.status.success());
    let config = fs::read_to_string(temp.path().join("quality.yml")).unwrap();
    assert!(config.contains("typecheck:"));
    assert!(config.contains("name: TypeScript"));
    assert!(config.contains("command: yarn"));
    assert!(config.contains("- type-check"));
    assert!(!config.contains("check: false"));
}

#[test]
fn init_dry_run_previews_without_writing_or_replacing_configuration() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("package.json"),
        r#"{"devDependencies":{"eslint":"9.0.0"}}"#,
    )
    .unwrap();
    fs::write(temp.path().join("quality.yml"), "existing: true\n").unwrap();

    let output = quality(temp.path(), &["init", "--dry-run"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("eslint:"));
    assert!(
        stdout.contains("$schema=https://quality-cli.santi020k.chatgpt.site/quality.schema.json")
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("quality.yml")).unwrap(),
        "existing: true\n"
    );
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
fn adapter_selection_typos_are_rejected_with_a_suggestion() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();

    let output = quality(temp.path(), &["check", "--only", "swfitlint"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown adapter `swfitlint`"));
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
fn agent_instructions_are_generated_without_project_discovery() {
    let missing_root = "/definitely/not/a/quality/project";
    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(missing_root)
        .args(["instructions", "--format", "agents"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("## Code quality\n"));
    assert!(stdout.contains("`quality check` before handoff"));
    assert!(stdout.contains("Do not bypass configured checks"));
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
    assert_eq!(report["summary"]["tools"], 1);
    assert_eq!(report["summary"]["warnings"], 1);
    assert_eq!(report["summary"]["files"][0], "App.swift");
    assert_eq!(report["summary"]["rules"]["example_rule"], 1);

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
fn severity_levels_separate_reporting_from_failure_and_write_a_summary() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
    let fake = temp.path().join("fake-swiftlint");
    fs::write(
        &fake,
        "#!/bin/sh\necho 'App.swift:3:1: warning: Non-blocking warning (example_rule)'\nexit 1\n",
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
    let summary = temp.path().join("summary.md");

    let output = Command::new(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(temp.path())
        .args([
            "check",
            "--format",
            "github",
            "--report-level",
            "warning",
            "--fail-level",
            "error",
        ])
        .env("GITHUB_STEP_SUMMARY", &summary)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("::warning file=App.swift"));
    let summary = fs::read_to_string(summary).unwrap();
    assert!(summary.contains("| SwiftLint | ⚠️ Findings | 1 |"));

    let strict = quality(temp.path(), &["check", "--fail-level", "warning"]);
    assert_eq!(strict.status.code(), Some(1));
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

#[cfg(unix)]
#[test]
fn changed_mode_runs_full_analyzer_when_configuration_is_deleted() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("app.ts"), "const value = 1;\n").unwrap();
    fs::write(temp.path().join("eslint.config.js"), "export default [];\n").unwrap();
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
    fs::remove_file(temp.path().join("eslint.config.js")).unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["scope"]["files"], 1);
    let command = report["results"][0]["command"].as_str().unwrap();
    assert!(command.ends_with(". --format json"));
    assert!(!command.contains("eslint.config.js"));
}

#[cfg(unix)]
#[test]
fn changed_mode_never_passes_deleted_sources_to_file_scoped_tools() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("live.ts"), "const live = 1;\n").unwrap();
    fs::write(temp.path().join("deleted.ts"), "const removed = 1;\n").unwrap();
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
    fs::write(temp.path().join("live.ts"), "const live = 2;\n").unwrap();
    fs::remove_file(temp.path().join("deleted.ts")).unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["scope"]["files"], 2);
    let command = report["results"][0]["command"].as_str().unwrap();
    assert!(command.ends_with("--format json live.ts"));
    assert!(!command.contains("deleted.ts"));
}

#[cfg(unix)]
#[test]
fn changed_astro_files_run_eslint_and_lockfiles_expand_to_full_scope() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("page.astro"), "<h1>Hello</h1>\n").unwrap();
    fs::write(
        temp.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();
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

    fs::write(temp.path().join("page.astro"), "<h1>Changed</h1>\n").unwrap();
    let astro = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert!(astro.status.success());
    let report: serde_json::Value = serde_json::from_slice(&astro.stdout).unwrap();
    assert!(
        report["results"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("--format json page.astro")
    );

    git(temp.path(), &["checkout", "--", "page.astro"]);
    fs::write(
        temp.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.1'\n",
    )
    .unwrap();
    let lockfile = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert!(lockfile.status.success());
    let report: serde_json::Value = serde_json::from_slice(&lockfile.stdout).unwrap();
    assert!(
        report["results"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with(". --format json")
    );
}

#[cfg(unix)]
#[test]
fn nested_android_workspace_uses_its_wrapper_and_rebases_diagnostics() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let android = temp.path().join("apps/android");
    fs::create_dir_all(android.join("src/main")).unwrap();
    fs::write(
        android.join("src/main/AndroidManifest.xml"),
        "<manifest />\n",
    )
    .unwrap();
    fs::write(android.join("src/main/Main.kt"), "class Main\n").unwrap();
    let wrapper = android.join("gradlew");
    fs::write(
        &wrapper,
        "#!/bin/sh\necho 'src/main/Main.kt:4:2: warning: Nested issue (android-rule)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&wrapper, permissions).unwrap();
    let fake_bin = temp.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let java = fake_bin.join("java");
    fs::write(&java, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&java).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&java, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  android-lint:\n    enabled: true\n  detekt:\n    enabled: false\n  ktlint:\n    enabled: false\n",
    )
    .unwrap();

    let doctor = quality_with_path(temp.path(), &["doctor", "--format", "json"], &fake_bin);
    assert!(doctor.status.success());
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let android_entry = report["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == "android-lint@apps/android")
        .unwrap();
    assert_eq!(android_entry["available"], true);
    assert!(
        android_entry["working_directory"]
            .as_str()
            .unwrap()
            .ends_with("apps/android")
    );

    let checked = quality(temp.path(), &["check", "--format", "json"]);
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["results"][0]["tool"], "android-lint@apps/android");
    assert_eq!(
        report["results"][0]["diagnostics"][0]["path"],
        "apps/android/src/main/Main.kt"
    );
}

#[cfg(unix)]
#[test]
fn repository_tasks_run_in_their_workspace_and_honor_change_filters() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    let workspace = temp.path().join("apps/web");
    fs::create_dir_all(workspace.join("src")).unwrap();
    fs::write(workspace.join("src/app.ts"), "export const value = 1;\n").unwrap();
    fs::write(workspace.join("README.md"), "Initial\n").unwrap();
    let command = workspace.join("check.sh");
    fs::write(
        &command,
        "#!/bin/sh\necho 'src/app.ts:2:1: error: Type mismatch (typecheck)'\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&command).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&command, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  eslint:\n    enabled: false\n  prettier:\n    enabled: false\ntasks:\n  typecheck:\n    name: TypeScript\n    command: ./check.sh\n    working_directory: apps/web\n    args: [--strict]\n    extensions: [ts]\n    config_files: [tsconfig.json]\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "initial"]);

    let doctor = quality(temp.path(), &["doctor"]);
    assert!(doctor.status.success());
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("TypeScript"));

    fs::write(workspace.join("README.md"), "Documentation only\n").unwrap();
    let skipped = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert!(skipped.status.success());
    let report: serde_json::Value = serde_json::from_slice(&skipped.stdout).unwrap();
    assert!(report["results"].as_array().unwrap().is_empty());

    fs::write(
        workspace.join("src/app.ts"),
        "export const value = 'changed';\n",
    )
    .unwrap();
    let checked = quality(temp.path(), &["check", "--changed", "--format", "json"]);
    assert_eq!(checked.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["results"][0]["tool"], "typecheck");
    assert_eq!(
        report["results"][0]["diagnostics"][0]["path"],
        "apps/web/src/app.ts"
    );
    assert!(
        report["results"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("./check.sh --strict")
    );
}

#[cfg(unix)]
#[test]
fn repository_tasks_run_when_a_relevant_source_is_deleted() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("app.ts"), "export const value = 1;\n").unwrap();
    let command = temp.path().join("typecheck.sh");
    fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&command).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&command, permissions).unwrap();
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools: {}\ntasks:\n  typecheck:\n    command: ./typecheck.sh\n    extensions: [ts]\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "initial"]);
    fs::remove_file(temp.path().join("app.ts")).unwrap();

    let output = quality(temp.path(), &["check", "--changed", "--format", "json"]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["results"][0]["tool"], "typecheck");
    assert_eq!(report["scope"]["files"], 1);
}

#[cfg(unix)]
#[test]
fn adapter_selection_filters_all_operations_and_reports_scope() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("widget.acme"), "content\n").unwrap();
    for id in ["alpha", "beta"] {
        let command = temp.path().join(id);
        fs::write(&command, "#!/bin/sh\nprintf '%s' \"$*\"\n").unwrap();
        let mut permissions = fs::metadata(&command).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(command, permissions).unwrap();
    }
    fs::write(
        temp.path().join("quality.yml"),
        "version: 1\noutput: pretty\ntools: {}\ncustom:\n  alpha:\n    command: ./alpha\n    extensions: [acme]\n    check_args: [check]\n    format_check_args: [format-check]\n    format_args: [format]\n    fix_args: [fix]\n  beta:\n    command: ./beta\n    extensions: [acme]\n    check_args: [check]\n    format_check_args: [format-check]\n    format_args: [format]\n    fix_args: [fix]\n",
    )
    .unwrap();

    let checked = quality(
        temp.path(),
        &["check", "--only", "alpha,alpha", "--format", "json"],
    );
    assert!(checked.status.success());
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    assert_eq!(report["results"][0]["tool"], "alpha");
    assert_eq!(report["scope"]["only"], serde_json::json!(["alpha"]));

    let formatted = quality(
        temp.path(),
        &["format", "--exclude", "beta", "--format", "json"],
    );
    assert!(formatted.status.success());
    let report: serde_json::Value = serde_json::from_slice(&formatted.stdout).unwrap();
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    assert_eq!(report["results"][0]["tool"], "alpha");
    assert_eq!(report["scope"]["exclude"], serde_json::json!(["beta"]));
    assert!(
        report["results"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("format")
    );

    let fixed = quality(temp.path(), &["fix", "--only", "beta", "--format", "sarif"]);
    assert!(fixed.status.success());
    let sarif: serde_json::Value = serde_json::from_slice(&fixed.stdout).unwrap();
    assert_eq!(sarif["runs"].as_array().unwrap().len(), 1);
    assert_eq!(sarif["runs"][0]["tool"]["driver"]["name"], "beta");
    assert_eq!(
        sarif["runs"][0]["properties"]["qualityScope"]["only"],
        serde_json::json!(["beta"])
    );
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
    assert!(workflow_text.contains("runs-on: ubuntu-latest"));
    assert!(workflow_text.contains("dtolnay/rust-toolchain"));
    assert!(!workflow_text.contains("__QUALITY_INSTALL_COMMAND__"));
    assert!(!workflow_text.contains("__QUALITY_RUNNER__"));
    assert!(!workflow_text.contains("__QUALITY_PROJECT_SETUP__"));
    let _: serde_yaml::Value = serde_yaml::from_str(&workflow_text).unwrap();

    let second = quality(temp.path(), &["ci", "github", "--install", install]);
    assert_eq!(second.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&second.stderr).contains("--force"));
}

#[test]
fn ci_generates_pnpm_setup_and_dependency_installation() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("package.json"), "{}\n").unwrap();
    fs::write(
        temp.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();

    let output = quality(
        temp.path(),
        &[
            "ci",
            "github",
            "--install",
            "curl -fsSL https://example.test/install | sh",
        ],
    );

    assert!(output.status.success());
    let workflow = fs::read_to_string(temp.path().join(".github/workflows/quality.yml")).unwrap();
    assert!(workflow.contains("pnpm/action-setup"));
    assert!(workflow.contains("cache: pnpm"));
    assert!(workflow.contains("pnpm install --frozen-lockfile"));
    assert!(!workflow.contains("dtolnay/rust-toolchain"));
    let _: serde_yaml::Value = serde_yaml::from_str(&workflow).unwrap();
}

#[test]
fn ci_uses_macos_and_installs_configured_swift_tools() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("Package.swift"),
        "// swift-tools-version: 6.0\n",
    )
    .unwrap();
    fs::write(temp.path().join("Sources.swift"), "struct App {}\n").unwrap();
    fs::write(temp.path().join(".swiftlint.yml"), "\n").unwrap();
    fs::write(temp.path().join(".swiftformat"), "\n").unwrap();

    let output = quality(
        temp.path(),
        &["ci", "github", "--install", "./install-quality"],
    );

    assert!(output.status.success());
    let workflow = fs::read_to_string(temp.path().join(".github/workflows/quality.yml")).unwrap();
    assert!(workflow.contains("runs-on: macos-latest"));
    assert!(workflow.contains("brew install swiftlint"));
    assert!(workflow.contains("brew install swiftformat"));
}

#[test]
fn ci_keeps_incidental_swift_sources_on_linux() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("NativeBridge.swift"),
        "struct NativeBridge {}\n",
    )
    .unwrap();

    let output = quality(
        temp.path(),
        &["ci", "github", "--install", "./install-quality"],
    );

    assert!(output.status.success());
    let workflow = fs::read_to_string(temp.path().join(".github/workflows/quality.yml")).unwrap();
    assert!(workflow.contains("runs-on: ubuntu-latest"));
}

#[test]
fn ci_installs_actionlint_when_the_repository_uses_it() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".github/workflows")).unwrap();
    fs::write(
        temp.path().join(".github/workflows/actionlint.yml"),
        "name: actionlint\n",
    )
    .unwrap();

    let output = quality(
        temp.path(),
        &["ci", "github", "--install", "./install-quality"],
    );

    assert!(output.status.success());
    let workflow = fs::read_to_string(temp.path().join(".github/workflows/quality.yml")).unwrap();
    assert!(workflow.contains("go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12"));
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
    fs::write(
        temp.path().join("package.json"),
        r#"{"devDependencies":{"eslint":"9.0.0","prettier":"3.0.0"}}"#,
    )
    .unwrap();
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

#[test]
fn repositories_audit_and_apply_emit_an_adoption_report() {
    let parent = tempfile::tempdir().unwrap();
    let configured = parent.path().join("configured");
    let missing = parent.path().join("missing");
    fs::create_dir_all(&configured).unwrap();
    fs::create_dir_all(&missing).unwrap();
    initialize_git(&configured);
    initialize_git(&missing);
    fs::write(
        configured.join("quality.yml"),
        "version: 1\noutput: pretty\ntools:\n  swiftlint:\n    enabled: true\n    required: true\n    command: definitely-not-a-real-tool\n",
    )
    .unwrap();
    fs::write(
        missing.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let audit = quality(
        parent.path(),
        &["repositories", "audit", "--format", "json"],
    );
    assert!(audit.status.success());
    let report: serde_json::Value = serde_json::from_slice(&audit.stdout).unwrap();
    assert_eq!(report["summary"]["total"], 2);
    assert_eq!(report["summary"]["needs_configuration"], 1);
    assert_eq!(report["summary"]["missing_toolchains"], 1);
    assert_eq!(
        report["repositories"][0]["missing_toolchains"][0],
        "swiftlint"
    );

    let pretty = quality(parent.path(), &["repositories", "audit"]);
    assert!(pretty.status.success());
    assert!(String::from_utf8_lossy(&pretty.stdout).contains("missing: swiftlint"));

    let applied = quality(
        parent.path(),
        &["repositories", "apply", "--format", "json"],
    );
    assert!(applied.status.success());
    let report: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(report["summary"]["created"], 1);
    assert!(missing.join("quality.yml").exists());
    assert!(configured.join("quality.yml").exists());
}
