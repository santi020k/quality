//! Regression coverage for analyzer output capture and parsing, baseline
//! safety, and local CI history retention and comparison.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ESLINT_ERROR: &str = r#"[{"filePath":"a.ts","messages":[{"ruleId":"semi","severity":2,"message":"Missing semicolon.","line":1,"column":1}]}]"#;
const ESLINT_WARNING: &str = r#"[{"filePath":"a.ts","messages":[{"ruleId":"no-console","severity":1,"message":"Unexpected console statement.","line":2,"column":3}]}]"#;
const NODE_WARNING: &str =
    "(node:42) ExperimentalWarning: Type Stripping is an experimental feature";

/// Hook steps export variables such as `QUALITY_LOCAL_CI` and `GIT_DIR`; clear
/// them so these tests behave the same when `cargo test` runs inside a hook.
fn quality(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quality"));
    command
        .arg("--root")
        .arg(root)
        .args(args)
        .env_remove("QUALITY_LOCAL_CI");
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(variable);
    }
    command.output().expect("quality should execute")
}

fn describe(output: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("expected JSON stdout: {error}\n{}", describe(output)))
}

fn message(result: &serde_json::Value) -> &str {
    result["diagnostics"][0]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a diagnostic message: {result}"))
}

fn write_task(root: &Path, script: &str, parser: &str) {
    let config = serde_json::json!({
        "version": 1,
        "tasks": {
            "lint": { "command": "sh", "args": ["-c", script], "parser": parser }
        }
    });
    fs::write(root.join("quality.yml"), config.to_string()).unwrap();
}

fn write_hook_step(root: &Path, script: &str, parser: &str) {
    let config = serde_json::json!({
        "version": 1,
        "hooks": {
            "pre-push": {
                "steps": [{ "name": "Lint", "command": "sh", "args": ["-c", script], "parser": parser }]
            }
        }
    });
    fs::write(root.join("quality.yml"), config.to_string()).unwrap();
}

fn print_line(text: &str) -> String {
    format!("printf '%s\\n' '{text}'")
}

#[test]
fn structured_findings_parse_from_stdout_despite_stderr_warnings() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        &format!(
            "{}; echo '{NODE_WARNING}' >&2; exit 1",
            print_line(ESLINT_ERROR)
        ),
        "eslint-json",
    );

    let output = quality(temp.path(), &["check", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let report = json(&output);
    let result = &report["results"][0];
    assert_eq!(result["failure_kind"], "code", "{result}");
    assert_eq!(
        result["diagnostics"].as_array().unwrap().len(),
        1,
        "{result}"
    );
    assert_eq!(result["diagnostics"][0]["path"], "a.ts");
    assert_eq!(result["diagnostics"][0]["rule"], "semi");
    assert!(
        result["output"]
            .as_str()
            .unwrap()
            .contains("ExperimentalWarning")
    );
}

#[test]
fn baselines_apply_to_structured_findings_reported_alongside_stderr_warnings() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        &format!(
            "{}; echo '{NODE_WARNING}' >&2; exit 1",
            print_line(ESLINT_ERROR)
        ),
        "eslint-json",
    );

    let created = quality(temp.path(), &["baseline", "create"]);
    assert!(created.status.success(), "{}", describe(&created));

    let checked = quality(temp.path(), &["check", "--format", "json"]);
    assert!(checked.status.success(), "{}", describe(&checked));
    let report = json(&checked);
    assert_eq!(report["suppressed"], 1);
    assert_eq!(report["results"][0]["status"], "passed");
    assert!(
        report["results"][0].get("failure_kind").is_none(),
        "a fully suppressed result is no longer a code failure: {report}"
    );
}

#[test]
fn successful_structured_output_keeps_warnings_despite_stderr_warnings() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        &format!("{}; echo '{NODE_WARNING}' >&2", print_line(ESLINT_WARNING)),
        "eslint-json",
    );

    let output = quality(temp.path(), &["check", "--format", "json"]);

    assert!(output.status.success(), "{}", describe(&output));
    let result = &json(&output)["results"][0];
    assert_eq!(result["status"], "passed");
    assert_eq!(result["diagnostics"][0]["path"], "a.ts");
    assert_eq!(result["diagnostics"][0]["severity"], "warning");
    assert_eq!(result["diagnostics"][0]["rule"], "no-console");
}

#[test]
fn valid_empty_structured_results_still_pass() {
    for (script, parser) in [
        (
            format!("printf '[]\\n'; echo '{NODE_WARNING}' >&2"),
            "eslint-json",
        ),
        ("printf '[\\n\\n]\\n'".to_owned(), "swiftlint-json"),
        ("printf '[\\n]\\n'".to_owned(), "ktlint-json"),
        ("exit 0".to_owned(), "typos-json"),
        (
            r#"printf '{"type":"binary_file","path":"./assets/logo.png"}\n'"#.to_owned(),
            "typos-json",
        ),
        ("exit 0".to_owned(), "codespell"),
        (
            r#"printf '{"command":"check","config":"og.config.mjs","stale":[]}\n'"#.to_owned(),
            "santi-og-json",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_task(temp.path(), &script, parser);

        let output = quality(temp.path(), &["check", "--format", "json"]);

        assert!(output.status.success(), "{parser}: {}", describe(&output));
        let result = &json(&output)["results"][0];
        assert_eq!(result["status"], "passed", "{parser}: {result}");
        assert_eq!(result["diagnostics"], serde_json::json!([]), "{parser}");
    }
}

#[test]
fn truncated_successful_structured_output_fails_and_never_becomes_a_baseline() {
    let temp = tempfile::tempdir().unwrap();
    write_task(temp.path(), &print_line(ESLINT_WARNING), "eslint-json");

    let output = quality(
        temp.path(),
        &["check", "--format", "json", "--max-output-bytes", "32"],
    );

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let result = &json(&output)["results"][0];
    assert_eq!(result["status"], "failed", "{result}");
    assert_eq!(result["failure_kind"], "toolchain");
    assert_eq!(result["output_truncated"], true);
    assert_eq!(result["diagnostics"][0]["rule"], "tool-output-unreadable");
    assert!(message(result).contains("--max-output-bytes"), "{result}");

    let baseline = quality(
        temp.path(),
        &["baseline", "create", "--max-output-bytes", "32"],
    );
    assert!(!baseline.status.success(), "{}", describe(&baseline));
    assert!(
        String::from_utf8_lossy(&baseline.stderr).contains("cannot create a safe baseline"),
        "{}",
        describe(&baseline)
    );
    assert!(!temp.path().join(".quality-baseline.json").exists());
}

#[test]
fn structured_findings_hidden_beyond_a_blank_capture_prefix_cannot_pass_or_be_baselined() {
    let temp = tempfile::tempdir().unwrap();
    let output_text = format!("{}{ESLINT_WARNING}", " ".repeat(80));
    write_task(temp.path(), &print_line(&output_text), "eslint-json");

    let checked = quality(
        temp.path(),
        &["check", "--format", "json", "--max-output-bytes", "32"],
    );
    assert_eq!(checked.status.code(), Some(1), "{}", describe(&checked));
    let result = &json(&checked)["results"][0];
    assert_eq!(result["status"], "failed");
    assert_eq!(result["failure_kind"], "toolchain");
    assert_eq!(result["output_truncated"], true);
    assert_eq!(result["diagnostics"][0]["rule"], "tool-output-unreadable");

    let baseline = quality(
        temp.path(),
        &["baseline", "create", "--max-output-bytes", "32"],
    );
    assert!(!baseline.status.success(), "{}", describe(&baseline));
    assert!(String::from_utf8_lossy(&baseline.stderr).contains("cannot create a safe baseline"));
    assert!(!temp.path().join(".quality-baseline.json").exists());

    // A successful generic command does not promise structured findings.
    write_task(temp.path(), &print_line(&output_text), "generic");
    let generic = quality(
        temp.path(),
        &["check", "--format", "json", "--max-output-bytes", "32"],
    );
    assert!(generic.status.success(), "{}", describe(&generic));
    assert_eq!(json(&generic)["results"][0]["status"], "passed");
}

#[test]
fn malformed_successful_structured_output_fails_as_a_toolchain_problem() {
    for script in [
        r#"printf '[{"filePath":'"#.to_owned(),
        // A package-script banner on stdout hides the findings that follow it.
        format!(
            "printf '\\n> app@1.0.0 lint\\n> eslint . --format json\\n\\n'; {}",
            print_line(ESLINT_WARNING)
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        write_task(temp.path(), &script, "eslint-json");

        let output = quality(temp.path(), &["check", "--format", "json"]);

        assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
        let result = &json(&output)["results"][0];
        assert_eq!(result["failure_kind"], "toolchain", "{result}");
        assert_eq!(result["diagnostics"][0]["rule"], "tool-output-unreadable");
        assert!(message(result).contains("eslint-json"), "{result}");
        assert!(!message(result).contains("filePath"), "{result}");
    }
}

#[test]
fn successful_output_in_another_format_keeps_the_generic_parser_convention() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        "printf 'src/a.ts\\n  1:1  warning  Unexpected console statement  no-console\\n'",
        "eslint-json",
    );

    let output = quality(temp.path(), &["check", "--format", "json"]);

    assert!(output.status.success(), "{}", describe(&output));
    assert_eq!(json(&output)["results"][0]["status"], "passed");
}

#[test]
fn truncated_failing_structured_output_reports_a_readable_bounded_message() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        &format!("{}; exit 1", print_line(ESLINT_ERROR)),
        "eslint-json",
    );

    let output = quality(
        temp.path(),
        &["check", "--format", "json", "--max-output-bytes", "32"],
    );

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let result = &json(&output)["results"][0];
    assert_eq!(result["status"], "failed");
    assert_eq!(result["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(result["diagnostics"][0]["rule"], "tool-output-unreadable");
    assert!(message(result).contains("--max-output-bytes"), "{result}");
    assert!(!message(result).contains("filePath"), "{result}");
}

#[test]
fn stdout_without_a_trailing_newline_does_not_merge_into_a_stderr_path() {
    let temp = tempfile::tempdir().unwrap();
    write_task(
        temp.path(),
        "printf checking; echo 'src/a.ts:1:2: error: boom (rule)' >&2; exit 1",
        "generic",
    );

    let output = quality(temp.path(), &["check", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let result = &json(&output)["results"][0];
    assert_eq!(
        result["diagnostics"].as_array().unwrap().len(),
        1,
        "{result}"
    );
    assert_eq!(result["diagnostics"][0]["path"], "src/a.ts");
    assert_eq!(result["diagnostics"][0]["line"], 1);
    assert_eq!(result["diagnostics"][0]["column"], 2);
}

#[test]
fn unparsed_failure_messages_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let long_line = "x".repeat(5000);
    write_task(
        temp.path(),
        &format!("{}; exit 1", print_line(&long_line)),
        "generic",
    );

    let output = quality(temp.path(), &["check", "--format", "json"]);

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let result = &json(&output)["results"][0];
    let text = message(result);
    assert!(text.starts_with("xxxx"), "{text}");
    assert!(text.ends_with('…'), "{text}");
    assert!(
        text.chars().count() <= 1001,
        "{} characters",
        text.chars().count()
    );
    assert!(result["output"].as_str().unwrap().contains(&long_line));
}

#[test]
fn local_ci_parses_structured_stdout_despite_stderr_warnings() {
    let temp = tempfile::tempdir().unwrap();
    write_hook_step(
        temp.path(),
        &format!(
            "{}; echo '{NODE_WARNING}' >&2; exit 1",
            print_line(ESLINT_ERROR)
        ),
        "eslint-json",
    );

    let output = quality(
        temp.path(),
        &["ci", "local", "--format", "json", "--no-history"],
    );

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let step = &json(&output)["steps"][0];
    assert_eq!(step["diagnostics"].as_array().unwrap().len(), 1, "{step}");
    assert_eq!(step["diagnostics"][0]["path"], "a.ts");
    assert_eq!(step["diagnostics"][0]["rule"], "semi");
}

#[test]
fn local_ci_keeps_stream_boundaries_and_drops_partial_leading_lines() {
    let temp = tempfile::tempdir().unwrap();
    write_hook_step(
        temp.path(),
        "printf checking; echo 'src/a.ts:1:2: error: boom (rule)' >&2; exit 1",
        "generic",
    );
    let output = quality(
        temp.path(),
        &["ci", "local", "--format", "json", "--no-history"],
    );
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let step = &json(&output)["steps"][0];
    assert_eq!(step["diagnostics"].as_array().unwrap().len(), 1, "{step}");
    assert_eq!(step["diagnostics"][0]["path"], "src/a.ts");

    // The 65-byte tail starts inside the first line: `rst.ts:1:1: …` is not a path.
    write_hook_step(
        temp.path(),
        "printf 'src/first.ts:1:1: error: first (rule)\\nsrc/b.ts:3:4: error: tail (rule)\\n' >&2; exit 1",
        "generic",
    );
    let output = quality(
        temp.path(),
        &[
            "ci",
            "local",
            "--format",
            "json",
            "--no-history",
            "--max-output-bytes",
            "65",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let step = &json(&output)["steps"][0];
    assert_eq!(step["output_truncated"], true, "{step}");
    assert_eq!(step["diagnostics"].as_array().unwrap().len(), 1, "{step}");
    assert_eq!(step["diagnostics"][0]["path"], "src/b.ts");
    assert!(
        step["output"].as_str().unwrap().starts_with("src/b.ts:3:4"),
        "{step}"
    );
}

fn initialize_git(root: &Path) {
    let output = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .output()
        .expect("git should run");
    assert!(output.status.success(), "{}", describe(&output));
}

fn history_directory(root: &Path) -> PathBuf {
    let directory = root.join(".git/quality/local-ci");
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn step_record(number: usize, name: &str, status: &str, duration_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "name": name,
        "status": status,
        "duration_ms": duration_ms,
        "command": format!("sh -c '{name}'"),
    })
}

fn write_run(directory: &Path, hook: &str, started_at_unix_ms: u64, steps: Vec<serde_json::Value>) {
    let duration_ms = steps
        .iter()
        .map(|step| step["duration_ms"].as_u64().unwrap())
        .sum::<u64>();
    let status = if steps.iter().any(|step| step["status"] == "failed") {
        "failed"
    } else {
        "passed"
    };
    let report = serde_json::json!({
        "schema_version": 1,
        "hook": hook,
        "started_at_unix_ms": started_at_unix_ms,
        "duration_ms": duration_ms,
        "status": status,
        "steps": steps,
    });
    fs::write(
        directory.join(format!("run-{started_at_unix_ms}.json")),
        report.to_string(),
    )
    .unwrap();
}

fn history_ids(root: &Path, hook: &str) -> Vec<String> {
    let output = quality(
        root,
        &[
            "ci", "history", "--hook", hook, "--limit", "100", "--format", "json",
        ],
    );
    assert!(output.status.success(), "{}", describe(&output));
    json(&output)["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn history_retention_is_independent_for_each_hook() {
    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(
        temp.path().join("quality.yml"),
        serde_json::json!({
            "version": 1,
            "hooks": {
                "commit-msg": { "steps": [{ "name": "Message", "command": "sh", "args": ["-c", "exit 0"] }] },
                "pre-push": { "steps": [{ "name": "Push", "command": "sh", "args": ["-c", "exit 0"] }] }
            }
        })
        .to_string(),
    )
    .unwrap();
    let directory = history_directory(temp.path());
    for started in [1_000, 1_001] {
        write_run(
            &directory,
            "pre-push",
            started,
            vec![step_record(1, "Push", "passed", 5)],
        );
    }
    for started in 2_000..2_020 {
        write_run(
            &directory,
            "commit-msg",
            started,
            vec![step_record(1, "Message", "passed", 1)],
        );
    }

    // A new commit-msg run prunes only that hook's oldest run.
    let output = quality(temp.path(), &["ci", "local", "--hook", "commit-msg"]);
    assert!(output.status.success(), "{}", describe(&output));

    let commit_runs = history_ids(temp.path(), "commit-msg");
    assert_eq!(commit_runs.len(), 20, "{commit_runs:?}");
    assert!(!commit_runs.contains(&"run-2000".to_owned()));
    assert_eq!(
        history_ids(temp.path(), "pre-push"),
        ["run-1001", "run-1000"]
    );
    let compared = quality(
        temp.path(),
        &["ci", "compare", "--hook", "pre-push", "--format", "json"],
    );
    assert!(compared.status.success(), "{}", describe(&compared));
}

#[test]
fn comparisons_never_measure_skipped_steps_and_default_to_comparable_runs() {
    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("quality.yml"), "version: 1\n").unwrap();
    let directory = history_directory(temp.path());
    let complete = |first: u64, second: u64| {
        vec![
            step_record(1, "Build", "passed", first),
            step_record(2, "Test", "passed", second),
        ]
    };
    let focused = |second: u64| {
        vec![
            step_record(1, "Build", "skipped", 0),
            step_record(2, "Test", "passed", second),
        ]
    };
    write_run(&directory, "pre-push", 1_000, complete(10, 20));
    write_run(&directory, "pre-push", 2_000, complete(12, 25));
    write_run(&directory, "pre-push", 3_000, focused(30));

    // The newest run is focused on step 2 and has no earlier focused peer.
    let rejected = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(!rejected.status.success(), "{}", describe(&rejected));
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("executed steps"),
        "{}",
        describe(&rejected)
    );

    // An explicit pair is honored, but a skipped step has no duration change.
    let explicit = quality(
        temp.path(),
        &["ci", "compare", "run-2000", "run-3000", "--format", "json"],
    );
    assert!(explicit.status.success(), "{}", describe(&explicit));
    let comparison = json(&explicit);
    assert_eq!(comparison["steps"][0]["name"], "Build");
    assert_eq!(comparison["steps"][0]["head_status"], "skipped");
    assert!(
        comparison["steps"][0]["duration_delta_ms"].is_null(),
        "{comparison}"
    );
    assert_eq!(comparison["steps"][1]["duration_delta_ms"], 5);

    write_run(&directory, "pre-push", 4_000, focused(28));
    let focused_pair = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(focused_pair.status.success(), "{}", describe(&focused_pair));
    let comparison = json(&focused_pair);
    assert_eq!(comparison["base"]["id"], "run-3000");
    assert_eq!(comparison["head"]["id"], "run-4000");
    assert!(
        comparison["steps"][0]["duration_delta_ms"].is_null(),
        "{comparison}"
    );
    assert_eq!(comparison["steps"][1]["duration_delta_ms"], -2);

    // A complete head skips focused runs when choosing its default base.
    write_run(&directory, "pre-push", 5_000, complete(11, 21));
    let complete_pair = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(
        complete_pair.status.success(),
        "{}",
        describe(&complete_pair)
    );
    let comparison = json(&complete_pair);
    assert_eq!(comparison["base"]["id"], "run-2000");
    assert_eq!(comparison["steps"][0]["duration_delta_ms"], -1);

    let pretty = quality(temp.path(), &["ci", "compare", "run-2000", "run-3000"]);
    assert!(pretty.status.success(), "{}", describe(&pretty));
    let text = String::from_utf8_lossy(&pretty.stdout);
    let build = text
        .lines()
        .find(|line| line.contains("Build"))
        .unwrap_or_else(|| panic!("missing Build comparison: {text}"));
    assert!(build.contains("-> skipped"), "{build}");
    assert!(build.contains("not compared"), "{build}");
}

#[test]
fn default_comparisons_match_actual_executed_commands_after_an_early_failure() {
    let temp = tempfile::tempdir().unwrap();
    initialize_git(temp.path());
    fs::write(temp.path().join("quality.yml"), "version: 1\n").unwrap();
    let directory = history_directory(temp.path());
    let complete = || {
        vec![
            step_record(1, "Build", "passed", 20),
            step_record(2, "Test", "passed", 30),
        ]
    };
    write_run(&directory, "pre-push", 1_000, complete());
    write_run(
        &directory,
        "pre-push",
        2_000,
        vec![
            step_record(1, "Build", "failed", 10),
            step_record(2, "Test", "skipped", 0),
        ],
    );

    // A focused first-step failure and an early full-run failure are indistinguishable.
    // Neither measured Test, so the completed whole hook is not a timing baseline.
    let rejected = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(!rejected.status.success(), "{}", describe(&rejected));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("executed steps"));

    write_run(
        &directory,
        "pre-push",
        3_000,
        vec![
            step_record(1, "Build", "passed", 12),
            step_record(2, "Test", "skipped", 0),
        ],
    );
    let focused = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(focused.status.success(), "{}", describe(&focused));
    assert_eq!(json(&focused)["base"]["id"], "run-2000");

    // Identical names do not make changed commands or directories comparable.
    let mut changed_command = complete();
    changed_command[1]["command"] = serde_json::json!("sh -c 'different test'");
    write_run(&directory, "pre-push", 4_000, changed_command);
    let changed = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(!changed.status.success(), "{}", describe(&changed));
    let mut changed_directory = complete();
    changed_directory[1]["working_directory"] = serde_json::json!("packages/app");
    write_run(&directory, "pre-push", 5_000, changed_directory);
    let changed = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(!changed.status.success(), "{}", describe(&changed));

    write_run(&directory, "pre-push", 6_000, complete());
    let matching = quality(temp.path(), &["ci", "compare", "--format", "json"]);
    assert!(matching.status.success(), "{}", describe(&matching));
    assert_eq!(json(&matching)["base"]["id"], "run-1000");
}
