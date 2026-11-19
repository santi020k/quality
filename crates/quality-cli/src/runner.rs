use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::changes::ChangeSet;
use crate::cli::{AdapterSelection, Severity};
use crate::config::{Config, DiagnosticParser};
use crate::process::AnalyzerProcess;
use crate::project::Project;
use crate::tools::{self, Invocation};

type CapturedOutput = (Vec<u8>, bool);

/// Captures a child's stdout/stderr on a background thread without ever
/// blocking the collector on that thread's `read`. A killed or exited
/// process can leave its pipe open if an inherited-fd descendant (e.g. a
/// background `sleep &` under `sh -c`) keeps writing to or simply holding
/// the same pipe, so the reader thread may never see EOF. `state` lets the
/// collector take a snapshot of whatever has been read so far instead of
/// joining the thread.
struct OutputReader {
    state: Arc<Mutex<CapturedOutput>>,
    completed: mpsc::Receiver<()>,
}

pub const REPORT_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Check,
    CheckFormat,
    Format,
    Fix,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
    Missing,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Code,
    Environment,
    Toolchain,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Diagnostic {
    pub tool: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub severity: String,
    pub message: String,
    pub rule: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub tool: String,
    pub name: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<FailureKind>,
    pub duration_ms: u128,
    pub command: String,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub output_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(skip)]
    pub baseline_safe: bool,
    #[serde(skip)]
    pub executed: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ExecutionSettings {
    pub jobs: usize,
    pub timeout_seconds: Option<u64>,
    pub max_output_bytes: usize,
    pub require_checks: bool,
}

impl Default for ExecutionSettings {
    fn default() -> Self {
        Self {
            jobs: thread::available_parallelism().map_or(1, usize::from),
            timeout_seconds: None,
            max_output_bytes: 1024 * 1024,
            require_checks: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    pub schema_version: u8,
    pub results: Vec<ToolResult>,
    pub summary: RunSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<RunScope>,
    #[serde(default)]
    pub suppressed: usize,
    #[serde(skip)]
    pub execution: ExecutionSettings,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RunSummary {
    pub tools: usize,
    pub passed: usize,
    pub failed: usize,
    pub missing: usize,
    pub diagnostics: usize,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub files: Vec<String>,
    pub rules: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip)]
    pub rerun_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

impl RunReport {
    pub fn new(
        results: Vec<ToolResult>,
        scope: Option<RunScope>,
        execution: ExecutionSettings,
    ) -> Self {
        let mut report = Self {
            schema_version: REPORT_SCHEMA_VERSION,
            results,
            summary: RunSummary::default(),
            scope,
            suppressed: 0,
            execution,
        };
        report.refresh_summary();
        report
    }

    pub fn refresh_summary(&mut self) {
        let mut files = BTreeSet::new();
        let mut rules = BTreeMap::new();
        let diagnostics = self
            .results
            .iter()
            .flat_map(|result| &result.diagnostics)
            .collect::<Vec<_>>();
        for diagnostic in &diagnostics {
            if let Some(path) = &diagnostic.path {
                files.insert(path.clone());
            }
            if let Some(rule) = &diagnostic.rule {
                *rules.entry(rule.clone()).or_insert(0) += 1;
            }
        }
        self.summary = RunSummary {
            tools: self.results.len(),
            passed: self
                .results
                .iter()
                .filter(|result| matches!(result.status, Status::Passed))
                .count(),
            failed: self
                .results
                .iter()
                .filter(|result| matches!(result.status, Status::Failed))
                .count(),
            missing: self
                .results
                .iter()
                .filter(|result| matches!(result.status, Status::Missing))
                .count(),
            diagnostics: diagnostics.len(),
            errors: diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity.eq_ignore_ascii_case("error"))
                .count(),
            warnings: diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity.eq_ignore_ascii_case("warning"))
                .count(),
            info: diagnostics
                .iter()
                .filter(|diagnostic| {
                    !diagnostic.severity.eq_ignore_ascii_case("error")
                        && !diagnostic.severity.eq_ignore_ascii_case("warning")
                })
                .count(),
            files: files.into_iter().collect(),
            rules,
        };
    }

    pub fn failed(&self) -> bool {
        self.results.iter().any(|result| {
            matches!(result.status, Status::Failed)
                || (matches!(result.status, Status::Missing) && result.guidance.is_some())
        })
    }

    pub fn failed_at(&self, level: Severity) -> bool {
        self.results
            .iter()
            .any(|result| Self::result_failed_at(result, level))
    }

    pub fn result_failed_at(result: &ToolResult, level: Severity) -> bool {
        (matches!(result.status, Status::Missing) && result.guidance.is_some())
            || (matches!(result.status, Status::Failed)
                && (!result.baseline_safe
                    || result.diagnostics.is_empty()
                    || result
                        .diagnostics
                        .iter()
                        .any(|diagnostic| level.includes(&diagnostic.severity))))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorEntry {
    pub tool: String,
    pub name: String,
    pub detected: bool,
    pub enabled: bool,
    pub check_enabled: bool,
    pub format_or_fix_available: bool,
    pub available: bool,
    pub required: bool,
    pub command: String,
    pub working_directory: String,
    pub guidance: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub schema_version: u8,
    pub root: String,
    pub config: String,
    pub tools: Vec<DoctorEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<crate::presets::PresetDoctorStatus>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.tools
            .iter()
            .any(|entry| entry.check_enabled && entry.required && !entry.available)
    }
}

pub fn doctor(project: &Project, config: &Config) -> DoctorReport {
    let mut entries: Vec<_> = tools::catalog()
        .into_iter()
        .flat_map(|tool| {
            let tool_config = config.tool(tool.id);
            let detected = tool.detect(project);
            let enabled = tool_config.enabled.unwrap_or(detected);
            let check_enabled = enabled && tool_config.check != Some(false);
            let format_or_fix_available = enabled && tool.supports_format_or_fix();
            let required = tool_config.required.unwrap_or(true);
            let invocations = tool.invocations(project, &tool_config, Operation::Check, None);
            if invocations.is_empty() {
                return vec![DoctorEntry {
                    tool: tool.id.to_owned(),
                    name: tool.name.to_owned(),
                    detected,
                    enabled,
                    check_enabled,
                    format_or_fix_available,
                    available: !check_enabled,
                    required,
                    command: tool.executable.to_owned(),
                    working_directory: project.root.display().to_string(),
                    guidance: None,
                }];
            }
            invocations
                .into_iter()
                .map(|invocation| {
                    let command_ready = command_available(
                            &invocation.working_directory,
                            Some(&invocation.command),
                        );
                    let runtime_ready = invocation.id.split('@').next() != Some("android-lint")
                        || java_runtime_available(&invocation.working_directory);
                    let available = !check_enabled || (command_ready && runtime_ready);
                    DoctorEntry {
                        tool: invocation.id,
                        name: invocation.name,
                        detected,
                        enabled,
                        check_enabled,
                        format_or_fix_available,
                        available,
                        required,
                        command: invocation.command.display().to_string(),
                        working_directory: invocation.working_directory.display().to_string(),
                        guidance: (check_enabled && !available).then(|| {
                            if command_ready && !runtime_ready {
                                "Install a Java runtime supported by the Android project and ensure `java -version` succeeds.".to_owned()
                            } else {
                                tool.install_hint.to_owned()
                            }
                        }),
                    }
                })
                .collect()
        })
        .collect();
    entries.extend(config.tasks.iter().map(|(id, task_config)| {
        let invocation = tools::task_invocation(id, task_config, project, Operation::Check, None)
            .expect("check tasks always create an invocation");
        let available = command_available(&invocation.working_directory, Some(&invocation.command));
        DoctorEntry {
            tool: invocation.id,
            name: invocation.name,
            detected: true,
            enabled: true,
            check_enabled: true,
            format_or_fix_available: false,
            available,
            required: task_config.required,
            command: invocation.command.display().to_string(),
            working_directory: invocation.working_directory.display().to_string(),
            guidance: (!available && task_config.required).then_some(invocation.install_hint),
        }
    }));
    entries.extend(config.custom_tools.iter().map(|(id, tool_config)| {
        let detected = tools::external_detects(project, tool_config);
        let enabled = tool_config.enabled && detected;
        let required = tool_config.required;
        let invocation =
            tools::external_invocation(id, tool_config, project, Operation::Check, None);
        let command = invocation
            .as_ref()
            .map(|value| value.command.display().to_string())
            .unwrap_or_else(|| tool_config.command.display().to_string());
        let available = !enabled
            || command_available(
                invocation
                    .as_ref()
                    .map(|value| value.working_directory.as_path())
                    .unwrap_or(&project.root),
                invocation.as_ref().map(|value| &value.command),
            );
        DoctorEntry {
            tool: id.clone(),
            name: tool_config.name.clone().unwrap_or_else(|| id.clone()),
            detected,
            enabled,
            check_enabled: enabled,
            format_or_fix_available: false,
            available,
            required,
            command,
            working_directory: invocation
                .as_ref()
                .map(|value| value.working_directory.display().to_string())
                .unwrap_or_else(|| project.root.display().to_string()),
            guidance: (enabled && !available).then(|| {
                tool_config.install_hint.clone().unwrap_or_else(|| {
                    format!("Install or configure the `{id}` command declared in quality.yml.")
                })
            }),
        }
    }));
    DoctorReport {
        schema_version: REPORT_SCHEMA_VERSION,
        root: project.root.display().to_string(),
        config: if project.root.join("quality.yml").exists() {
            "quality.yml".to_owned()
        } else {
            "automatic defaults (run `quality init` to make them explicit)".to_owned()
        },
        tools: entries,
        preset: crate::presets::doctor_status(project),
    }
}

pub fn execute(
    project: &Project,
    config: &Config,
    operation: Operation,
    fail_fast: bool,
    changes: Option<&ChangeSet>,
    selection: &AdapterSelection,
    settings: ExecutionSettings,
) -> Result<RunReport> {
    if settings.require_checks
        && collect_invocations(project, config, operation, None, selection).is_empty()
    {
        anyhow::bail!(
            "no configured adapters can run this operation; configure a check or remove `--require-checks`"
        );
    }
    let invocations = collect_invocations(project, config, operation, changes, selection);
    let scope = (changes.is_some() || !selection.is_empty()).then(|| RunScope {
        mode: changes.map(|_| "changed"),
        base: changes.map(|changes| changes.base.clone()),
        rerun_base: changes.map(|changes| changes.resolved_base.clone()),
        files: changes.map(|changes| changes.files.len()),
        only: selection.only.clone(),
        exclude: selection.exclude.clone(),
    });

    if fail_fast {
        let mut results = Vec::new();
        for invocation in invocations {
            let result = run_one(&project.root, invocation, settings);
            let failed = RunReport::result_failed_at(&result, Severity::Info);
            results.push(result);
            if failed {
                break;
            }
        }
        return Ok(RunReport::new(results, scope, settings));
    }

    let count = invocations.len();
    if count == 0 {
        return Ok(RunReport::new(Vec::new(), scope, settings));
    }
    let queue = Arc::new(Mutex::new(VecDeque::from(invocations)));
    let (sender, receiver) = mpsc::channel();
    for _ in 0..count.min(settings.jobs.max(1)) {
        let sender = sender.clone();
        let root = project.root.clone();
        let queue = Arc::clone(&queue);
        thread::spawn(move || {
            loop {
                let invocation = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some(invocation) = invocation else {
                    break;
                };
                if sender.send(run_one(&root, invocation, settings)).is_err() {
                    break;
                }
            }
        });
    }
    drop(sender);
    let mut results: Vec<_> = receiver.iter().take(count).collect();
    results.sort_by(|left, right| left.tool.cmp(&right.tool));
    Ok(RunReport::new(results, scope, settings))
}

/// Build a read-only preview through the same invocation builder used by execution.
pub fn print_check_plan(
    project: &Project,
    config: &Config,
    changes: Option<&ChangeSet>,
    selection: &AdapterSelection,
    require_checks: bool,
    explain: bool,
) -> Result<()> {
    let candidates = collect_invocations(
        project,
        config,
        Operation::Check,
        None,
        &AdapterSelection::default(),
    );
    if require_checks
        && !candidates.iter().any(|invocation| {
            selection.includes(invocation.id.split('@').next().unwrap_or(&invocation.id))
        })
    {
        anyhow::bail!(
            "no configured adapters can run this operation; configure a check or remove `--require-checks`"
        );
    }
    let selected = collect_invocations(project, config, Operation::Check, changes, selection);
    println!("Check plan (preview only; analyzers are not executed)");
    if let Some(changes) = changes {
        println!("Base: {}", changes.base);
        if explain {
            println!("Changed inputs (including deletions):");
            for path in &changes.files {
                println!(
                    "  {:?}{}",
                    path,
                    if changes.is_deleted(path) {
                        " (deleted)"
                    } else {
                        ""
                    }
                );
            }
        }
    }
    for candidate in &candidates {
        let adapter = candidate.id.split('@').next().unwrap_or(&candidate.id);
        let planned = selected
            .iter()
            .find(|invocation| invocation.id == candidate.id);
        if let Some(invocation) = planned {
            println!("\nRUN {}", invocation.id);
            println!("  Directory: {:?}", invocation.working_directory);
            println!("  Executable: {:?}", invocation.command);
            println!("  Arguments: {:?}", invocation.args);
            if !invocation.env.is_empty() {
                println!("  Environment: {:?}", invocation.env);
            }
            if explain {
                let reason = if changes.is_none() {
                    "Enabled check; full configured scope."
                } else if invocation.args != candidate.args || invocation.env != candidate.env {
                    "Changed-file scope; arguments or environment restrict the input."
                } else {
                    "Applicable changed inputs; full configured scope (project analyzer, configuration change, or command override)."
                };
                println!("  Reason: {reason}");
            }
        } else if explain {
            let reason = if selection.exclude.iter().any(|id| id == adapter) {
                "Excluded by --exclude."
            } else if !selection.includes(adapter) {
                "Not selected by --only."
            } else {
                "No applicable changed inputs; deleted source files are not passed to file-scoped tools."
            };
            println!("\nSKIP {}\n  Reason: {reason}", candidate.id);
        }
    }
    if explain {
        for tool in tools::catalog() {
            let policy = config.tool(tool.id);
            let reason = if policy.enabled == Some(false) {
                Some("Disabled in configuration.")
            } else if policy.enabled.is_none() && !tool.detect(project) {
                Some("No repository intent detected.")
            } else if policy.check == Some(false) {
                Some("Check disabled; formatting or fixing may remain available.")
            } else {
                None
            };
            if let Some(reason) = reason {
                println!("\nSKIP {}\n  Reason: {reason}", tool.id);
            }
        }
        for (id, policy) in &config.custom_tools {
            if !policy.enabled || !tools::external_detects(project, policy) {
                println!(
                    "\nSKIP {id}\n  Reason: {}",
                    if policy.enabled {
                        "No configured source extensions detected."
                    } else {
                        "Disabled in configuration."
                    }
                );
            }
        }
    }
    println!(
        "\n{} check command(s) planned. A preview does not verify tool availability or check results.",
        selected.len()
    );
    Ok(())
}

fn collect_invocations(
    project: &Project,
    config: &Config,
    operation: Operation,
    changes: Option<&ChangeSet>,
    selection: &AdapterSelection,
) -> Vec<Invocation> {
    let mut invocations: Vec<_> = tools::catalog()
        .into_iter()
        .flat_map(|tool| tool.invocations(project, &config.tool(tool.id), operation, changes))
        .collect();
    invocations.extend(config.tasks.iter().filter_map(|(id, task_config)| {
        tools::task_invocation(id, task_config, project, operation, changes)
    }));
    invocations.extend(config.custom_tools.iter().filter_map(|(id, tool_config)| {
        tools::external_invocation(id, tool_config, project, operation, changes)
    }));
    invocations.retain(|invocation| {
        let adapter = invocation.id.split('@').next().unwrap_or(&invocation.id);
        selection.includes(adapter)
    });
    invocations
}

fn run_one(
    root: &std::path::Path,
    invocation: Invocation,
    settings: ExecutionSettings,
) -> ToolResult {
    let started = Instant::now();
    let command_display = format_command(&invocation);
    let timeout = settings.timeout_seconds.or(invocation.timeout_seconds);
    let mut command = Command::new(&invocation.command);
    command
        .args(&invocation.args)
        .envs(&invocation.env)
        .current_dir(&invocation.working_directory)
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = AnalyzerProcess::spawn(command, timeout.is_some());

    match child {
        Ok(mut child) => {
            let stdout = child
                .take_stdout()
                .map(|stdout| spawn_output_reader(stdout, settings.max_output_bytes));
            let stderr = child
                .take_stderr()
                .map(|stderr| spawn_output_reader(stderr, settings.max_output_bytes));
            let mut cleanup_error = None;
            let (status, timed_out) = match timeout {
                Some(seconds) => match child.wait_timeout(Duration::from_secs(seconds)) {
                    Ok(Some(status)) => (Ok(status), false),
                    Ok(None) => {
                        cleanup_error = child.terminate().err();
                        let status =
                            child
                                .wait_timeout(Duration::from_secs(1))
                                .and_then(|status| {
                                    status.ok_or_else(|| {
                                        io::Error::new(
                                            io::ErrorKind::TimedOut,
                                            "analyzer did not exit after termination",
                                        )
                                    })
                                });
                        if let Err(error) = &status {
                            cleanup_error
                                .get_or_insert_with(|| io::Error::other(error.to_string()));
                        }
                        (status, true)
                    }
                    Err(error) => (Err(error), false),
                },
                None => (child.wait(), false),
            };
            let stdout = collect_output(stdout);
            let stderr = collect_output(stderr);
            let limit = settings.max_output_bytes;
            let (combined, output_truncated) = combine_limited_output(&stdout, &stderr, limit);
            if timed_out {
                let seconds = timeout.unwrap_or_default();
                return ToolResult {
                    tool: invocation.id.clone(),
                    name: invocation.name,
                    status: Status::Failed,
                    failure_kind: Some(FailureKind::Environment),
                    duration_ms: started.elapsed().as_millis(),
                    command: command_display,
                    diagnostics: vec![Diagnostic {
                        tool: invocation.id,
                        path: None,
                        line: None,
                        column: None,
                        severity: "error".to_owned(),
                        message: format!("tool exceeded its {seconds}-second timeout"),
                        rule: Some("tool-timeout".to_owned()),
                    }],
                    output: display_output(combined, output_truncated, limit),
                    output_truncated,
                    guidance: cleanup_error
                        .map(|error| format!("could not terminate analyzer processes: {error}")),
                    baseline_safe: false,
                    executed: true,
                };
            }
            let status = match status {
                Ok(status) => status,
                Err(error) => return execution_error(invocation, command_display, started, error),
            };
            let stdout_text = String::from_utf8_lossy(&stdout.0);
            let stderr_text = String::from_utf8_lossy(&stderr.0);
            let parsed = parse_output(
                invocation.parser,
                &invocation.id,
                root,
                &invocation.working_directory,
                &ToolOutput {
                    stdout: &stdout_text,
                    stdout_capture: stream_capture(&stdout, limit),
                    stderr: &stderr_text,
                    combined: &combined,
                },
                !status.success(),
            );
            let (result_status, failure_kind) = if !status.success() {
                let kind = if parsed.fully_parsed {
                    FailureKind::Code
                } else if parsed.unreadable {
                    // Unreadable structured output is not evidence of its contents.
                    classify_failure(&stderr_text)
                } else {
                    classify_failure(&combined)
                };
                (Status::Failed, Some(kind))
            } else if parsed.unreadable {
                // A successful exit cannot vouch for findings that could not be read.
                (Status::Failed, Some(FailureKind::Toolchain))
            } else {
                (Status::Passed, None)
            };
            // A truncated or unfinished capture never proves the output is complete,
            // even when the retained prefix happens to parse cleanly on its own.
            let baseline_safe = parsed.fully_parsed && !parsed.unreadable && !output_truncated;
            ToolResult {
                tool: invocation.id,
                name: invocation.name,
                status: result_status,
                failure_kind,
                duration_ms: started.elapsed().as_millis(),
                command: command_display,
                diagnostics: parsed.diagnostics,
                output: display_output(combined, output_truncated, limit),
                output_truncated,
                guidance: None,
                baseline_safe,
                executed: true,
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let guidance = invocation.required.then_some(invocation.install_hint);
            let diagnostics = guidance
                .as_ref()
                .map(|message| {
                    vec![Diagnostic {
                        tool: invocation.id.clone(),
                        path: None,
                        line: None,
                        column: None,
                        severity: "error".to_owned(),
                        message: message.clone(),
                        rule: Some("tool-not-installed".to_owned()),
                    }]
                })
                .unwrap_or_default();
            ToolResult {
                tool: invocation.id,
                name: invocation.name,
                status: Status::Missing,
                failure_kind: Some(FailureKind::Toolchain),
                duration_ms: started.elapsed().as_millis(),
                command: command_display,
                diagnostics,
                output: String::new(),
                output_truncated: false,
                guidance,
                baseline_safe: false,
                executed: false,
            }
        }
        Err(error) => ToolResult {
            tool: invocation.id.clone(),
            name: invocation.name,
            status: Status::Failed,
            failure_kind: Some(FailureKind::Environment),
            duration_ms: started.elapsed().as_millis(),
            command: command_display,
            diagnostics: vec![Diagnostic {
                tool: invocation.id,
                path: None,
                line: None,
                column: None,
                severity: "error".to_owned(),
                message: error.to_string(),
                rule: None,
            }],
            output: error.to_string(),
            output_truncated: false,
            guidance: None,
            baseline_safe: false,
            executed: false,
        },
    }
}

fn execution_error(
    invocation: Invocation,
    command_display: String,
    started: Instant,
    error: io::Error,
) -> ToolResult {
    ToolResult {
        tool: invocation.id.clone(),
        name: invocation.name,
        status: Status::Failed,
        failure_kind: Some(FailureKind::Environment),
        duration_ms: started.elapsed().as_millis(),
        command: command_display,
        diagnostics: vec![Diagnostic {
            tool: invocation.id,
            path: None,
            line: None,
            column: None,
            severity: "error".to_owned(),
            message: error.to_string(),
            rule: None,
        }],
        output: error.to_string(),
        output_truncated: false,
        guidance: None,
        baseline_safe: false,
        executed: true,
    }
}

fn spawn_output_reader(mut reader: impl Read + Send + 'static, limit: usize) -> OutputReader {
    let state = Arc::new(Mutex::new((Vec::new(), false)));
    let thread_state = Arc::clone(&state);
    let (completed_tx, completed) = mpsc::channel();
    thread::spawn(move || {
        read_limited(&mut reader, limit, &thread_state);
        let _ = completed_tx.send(());
    });
    OutputReader { state, completed }
}

fn read_limited(reader: &mut impl Read, limit: usize, state: &Mutex<CapturedOutput>) {
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            if let Ok(mut captured) = state.lock() {
                captured.1 = true;
            }
            break;
        };
        if read == 0 {
            break;
        }
        if let Ok(mut captured) = state.lock() {
            let remaining = limit.saturating_sub(captured.0.len());
            let keep = read.min(remaining);
            captured.0.extend_from_slice(&buffer[..keep]);
            captured.1 |= keep < read;
        }
    }
}

/// Takes a snapshot of the captured output without blocking on the reader
/// thread, which may still be stuck in a blocking `read` on a pipe an
/// inherited-fd descendant keeps open well past the parent process's exit.
/// Any capture that hasn't finished within the grace window is marked
/// truncated, since there is no way to know whether more output was coming.
fn collect_output(handle: Option<OutputReader>) -> CapturedOutput {
    let Some(handle) = handle else {
        return (Vec::new(), false);
    };
    let completed = handle
        .completed
        .recv_timeout(Duration::from_millis(250))
        .is_ok();
    let mut captured = handle
        .state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_else(|_| (Vec::new(), true));
    captured.1 |= !completed;
    captured
}

/// Keeps the first `limit` captured bytes, preferring standard output.
fn combine_limited_output(
    stdout: &CapturedOutput,
    stderr: &CapturedOutput,
    limit: usize,
) -> (String, bool) {
    let stdout_keep = stdout.0.len().min(limit);
    let stderr_keep = stderr.0.len().min(limit - stdout_keep);
    let truncated = stdout.1 || stderr.1 || stdout.0.len() + stderr.0.len() > limit;
    (
        join_streams(&stdout.0[..stdout_keep], &stderr.0[..stderr_keep]),
        truncated,
    )
}

/// Joins captured streams, ending an unterminated standard output line so it
/// cannot merge with the first standard error line, such as a file path. The
/// separator is not captured output and does not count toward the limit.
pub(crate) fn join_streams(stdout: &[u8], stderr: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(stdout.len() + stderr.len() + 1);
    bytes.extend_from_slice(stdout);
    if !stdout.is_empty() && !stderr.is_empty() && !stdout.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(stderr);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Appends the truncation notice shown to people; parsers never see it.
fn display_output(mut output: String, truncated: bool, limit: usize) -> String {
    if truncated {
        output.push_str(&format!(
            "\n[quality: output truncated after {limit} bytes]\n"
        ));
    }
    output
}

fn stream_capture(captured: &CapturedOutput, limit: usize) -> StreamCapture {
    if !captured.1 {
        StreamCapture::Complete
    } else if captured.0.len() >= limit {
        StreamCapture::Truncated(limit)
    } else {
        StreamCapture::Unfinished
    }
}

/// Whether a captured stream contains everything the command wrote to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamCapture {
    Complete,
    /// Output beyond this many bytes was discarded.
    Truncated(usize),
    /// Reading failed or had not finished when collection stopped.
    Unfinished,
}

/// One command's captured output. Structured parsers read standard output
/// alone, so warnings on standard error cannot corrupt a machine-readable
/// document; the generic parser and failure messages use both streams.
pub(crate) struct ToolOutput<'a> {
    pub stdout: &'a str,
    pub stdout_capture: StreamCapture,
    pub stderr: &'a str,
    /// Both bounded streams joined by [`join_streams`], without display notices.
    pub combined: &'a str,
}

pub(crate) struct ParsedOutput {
    pub diagnostics: Vec<Diagnostic>,
    /// Every finding in the output is represented by a diagnostic.
    pub fully_parsed: bool,
    /// Standard output resembles the declared structured format or its capture
    /// is incomplete, and the findings could not be read completely. A successful
    /// exit cannot vouch for findings that were never read.
    pub unreadable: bool,
}

const UNREADABLE_OUTPUT_RULE: &str = "tool-output-unreadable";
/// Longest message, in characters, synthesized from unparsed failure output.
const UNPARSED_MESSAGE_LIMIT: usize = 1000;

pub(crate) fn parse_output(
    parser: DiagnosticParser,
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &ToolOutput<'_>,
    synthesize_failure: bool,
) -> ParsedOutput {
    let structured = parse_structured(parser, tool, root, working_directory, output.stdout);
    let resembles_structured = resembles_structured_output(parser, output.stdout);
    let parsed_structured = structured.is_some();
    // A bounded prefix can stop before the first JSON token (for example, in
    // leading whitespace). Only a parsed, closed document proves completeness
    // when a declared structured stream was not captured in full.
    let unreadable = resembles_structured && !parsed_structured
        || parser != DiagnosticParser::Generic
            && output.stdout_capture != StreamCapture::Complete
            && !(parsed_structured && self_delimiting(parser, output.stdout));
    if let Some(mut diagnostics) = structured {
        // A failed run without structured findings usually explains itself on
        // standard error, which the generic fallback below reports instead.
        // Annotation streams interleave free-form summaries, so a failed run
        // without annotations may explain itself on either stream.
        let explanation = if parser == DiagnosticParser::GithubAnnotations {
            output.combined
        } else {
            output.stderr
        };
        if !(synthesize_failure && diagnostics.is_empty() && has_meaningful_line(explanation)) {
            if unreadable {
                diagnostics.push(unreadable_output_diagnostic(
                    tool,
                    parser,
                    output.stdout_capture,
                ));
            }
            return ParsedOutput {
                diagnostics,
                fully_parsed: true,
                unreadable,
            };
        }
    }
    if parser == DiagnosticParser::Generic && !synthesize_failure {
        return ParsedOutput {
            diagnostics: Vec::new(),
            fully_parsed: false,
            unreadable: false,
        };
    }

    // Fragments of an unreadable structured document are not diagnostics.
    let text = if unreadable {
        output.stderr
    } else {
        output.combined
    };
    let pattern = diagnostic_pattern();
    let mut diagnostics = Vec::new();
    let meaningful_lines: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    for line in &meaningful_lines {
        let Some(captures) = pattern.captures(line.trim()) else {
            continue;
        };
        // Positions are one-based; tools print 0 for findings without a location.
        let position = |name: &str| {
            captures
                .name(name)
                .and_then(|value| value.as_str().parse::<u64>().ok())
                .filter(|position| *position > 0)
        };
        let source_line = position("line");
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: captures
                .name("path")
                .map(|value| normalize_path(root, working_directory, value.as_str())),
            line: source_line,
            column: source_line.and(position("column")),
            severity: match captures.name("severity").map(|value| value.as_str()) {
                // The report contract has no style level; style findings are informational.
                Some("style") => "info".to_owned(),
                Some(severity) => severity.to_owned(),
                None => "warning".to_owned(),
            },
            message: captures
                .name("message")
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| (*line).to_owned()),
            rule: captures.name("rule").map(|value| value.as_str().to_owned()),
        });
    }
    let fully_parsed =
        !unreadable && !diagnostics.is_empty() && diagnostics.len() == meaningful_lines.len();
    if unreadable {
        diagnostics.push(unreadable_output_diagnostic(
            tool,
            parser,
            output.stdout_capture,
        ));
    } else if diagnostics.is_empty() && synthesize_failure {
        // Valid structured output without findings leaves standard error as the
        // explanation; annotation streams also carry free-form errors on stdout.
        let explanation = if parsed_structured && parser != DiagnosticParser::GithubAnnotations {
            output.stderr
        } else {
            output.combined
        };
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: None,
            line: None,
            column: None,
            severity: "error".to_owned(),
            message: unparsed_failure_message(explanation),
            rule: None,
        });
    }
    ParsedOutput {
        diagnostics,
        fully_parsed,
        unreadable,
    }
}

fn parse_structured(
    parser: DiagnosticParser,
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    stdout: &str,
) -> Option<Vec<Diagnostic>> {
    match parser {
        DiagnosticParser::Codespell => parse_codespell(tool, root, working_directory, stdout),
        DiagnosticParser::EslintJson => parse_eslint_json(tool, root, working_directory, stdout),
        DiagnosticParser::SwiftlintJson => {
            parse_swiftlint_json(tool, root, working_directory, stdout)
        }
        DiagnosticParser::KtlintJson => parse_ktlint_json(tool, root, working_directory, stdout),
        DiagnosticParser::SantiOgJson => parse_santi_og_json(tool, root, working_directory, stdout),
        DiagnosticParser::TyposJson => parse_typos_json(tool, root, working_directory, stdout),
        DiagnosticParser::GithubAnnotations => {
            parse_github_annotations(tool, root, working_directory, stdout)
        }
        DiagnosticParser::Generic => None,
    }
}

/// Whether standard output contains a line that starts like the parser's JSON
/// format, including JSON preceded by a script banner. Output in any other
/// format, such as a formatter selected by custom arguments, keeps the generic
/// fallback instead of being treated as a damaged document.
fn resembles_structured_output(parser: DiagnosticParser, stdout: &str) -> bool {
    match parser {
        DiagnosticParser::EslintJson
        | DiagnosticParser::SwiftlintJson
        | DiagnosticParser::KtlintJson
        | DiagnosticParser::SantiOgJson
        | DiagnosticParser::TyposJson => stdout
            .lines()
            .any(|line| line.trim_start().starts_with(['[', '{'])),
        DiagnosticParser::Codespell
        | DiagnosticParser::GithubAnnotations
        | DiagnosticParser::Generic => false,
    }
}

/// Whether a successful structured parse proves standard output ended even
/// though its capture did not: a JSON document closes explicitly, while
/// line-oriented output may have stopped at any line boundary.
fn self_delimiting(parser: DiagnosticParser, stdout: &str) -> bool {
    match parser {
        DiagnosticParser::EslintJson
        | DiagnosticParser::SwiftlintJson
        | DiagnosticParser::SantiOgJson => true,
        DiagnosticParser::KtlintJson => stdout.trim_start().starts_with('['),
        DiagnosticParser::TyposJson
        | DiagnosticParser::Codespell
        | DiagnosticParser::GithubAnnotations
        | DiagnosticParser::Generic => false,
    }
}

fn has_meaningful_line(text: &str) -> bool {
    text.lines().any(|line| !line.trim().is_empty())
}

pub(crate) fn unparsed_failure_message(output: &str) -> String {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("tool exited unsuccessfully");
    match line.char_indices().nth(UNPARSED_MESSAGE_LIMIT) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line.to_owned(),
    }
}

fn unreadable_output_diagnostic(
    tool: &str,
    parser: DiagnosticParser,
    capture: StreamCapture,
) -> Diagnostic {
    let format = parser_name(parser);
    let message = match capture {
        StreamCapture::Truncated(limit) => format!(
            "{format} output exceeded the {limit}-byte capture limit, so its findings could not be read; rerun with a larger --max-output-bytes"
        ),
        StreamCapture::Unfinished => format!(
            "{format} output capture did not finish, so its findings could not be read; make sure no background process keeps standard output open"
        ),
        StreamCapture::Complete => format!(
            "standard output is not valid {format} output, so its findings could not be read; make sure the command writes only {format} to standard output"
        ),
    };
    Diagnostic {
        tool: tool.to_owned(),
        path: None,
        line: None,
        column: None,
        severity: "error".to_owned(),
        message,
        rule: Some(UNREADABLE_OUTPUT_RULE.to_owned()),
    }
}

/// The `parser` value accepted in `quality.yml`.
fn parser_name(parser: DiagnosticParser) -> &'static str {
    match parser {
        DiagnosticParser::Generic => "generic",
        DiagnosticParser::Codespell => "codespell",
        DiagnosticParser::EslintJson => "eslint-json",
        DiagnosticParser::SwiftlintJson => "swiftlint-json",
        DiagnosticParser::KtlintJson => "ktlint-json",
        DiagnosticParser::SantiOgJson => "santi-og-json",
        DiagnosticParser::TyposJson => "typos-json",
        DiagnosticParser::GithubAnnotations => "github-annotations",
    }
}

fn diagnostic_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"^(?P<path>.*?):(?P<line>\d+)(?::(?P<column>\d+))?:\s*(?:(?P<severity>warning|error|info|style):\s*)?(?P<message>.*?)(?:\s+\((?P<rule>[^()]+)\))?$",
        )
        .expect("valid diagnostic regular expression")
    })
}

fn parse_codespell(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"^(?P<path>.*?):(?P<line>\d+):\s*(?P<typo>\S+)\s+==>\s+(?P<corrections>.+)$")
            .expect("valid Codespell diagnostic regular expression")
    });
    let lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Some(Vec::new());
    }
    let mut diagnostics = Vec::with_capacity(lines.len());
    for line in lines {
        let captures = pattern.captures(line)?;
        let typo = captures.name("typo")?.as_str();
        let corrections = captures.name("corrections")?.as_str();
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: captures
                .name("path")
                .map(|value| normalize_path(root, working_directory, value.as_str())),
            line: captures
                .name("line")
                .and_then(|value| value.as_str().parse().ok()),
            column: None,
            severity: "warning".to_owned(),
            message: format!("Possible misspelling `{typo}`; suggested: {corrections}"),
            rule: Some("spelling".to_owned()),
        });
    }
    Some(diagnostics)
}

fn parse_typos_json(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let mut diagnostics = Vec::with_capacity(lines.len());
    for line in lines {
        let finding = serde_json::from_str::<serde_json::Value>(line).ok()?;
        match finding.get("type").and_then(serde_json::Value::as_str) {
            Some("typo") => {}
            // Typos reports each skipped binary file; it is not a finding.
            Some("binary_file") => continue,
            _ => return None,
        }
        let path = finding.get("path")?.as_str()?;
        let typo = finding.get("typo")?.as_str()?;
        let corrections = finding
            .get("corrections")
            .and_then(serde_json::Value::as_array)?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        let suggestion = if corrections.is_empty() {
            "no unambiguous correction".to_owned()
        } else {
            format!("suggested: {}", corrections.join(", "))
        };
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: Some(normalize_path(root, working_directory, path)),
            line: finding.get("line_num").and_then(serde_json::Value::as_u64),
            column: finding
                .get("byte_offset")
                .and_then(serde_json::Value::as_u64)
                .map(|offset| offset.saturating_add(1)),
            severity: "warning".to_owned(),
            message: format!("Possible misspelling `{typo}`; {suggestion}"),
            rule: Some("spelling".to_owned()),
        });
    }
    Some(diagnostics)
}

/// Reads GitHub workflow-command annotations (`::error file=…,line=…::message`),
/// which Biome and Oxlint emit with their `github` reporters. Other lines are
/// summaries and progress output, not findings.
fn parse_github_annotations(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    for line in output.lines().map(str::trim) {
        let Some(command) = line.strip_prefix("::") else {
            continue;
        };
        let Some((header, message)) = command.split_once("::") else {
            continue;
        };
        let (level, properties) = header.split_once(' ').unwrap_or((header, ""));
        let severity = match level {
            "error" => "error",
            "warning" => "warning",
            "notice" => "info",
            // Other workflow commands, such as `::group::`, are not findings.
            _ => continue,
        };
        let mut path = None;
        let mut source_line = None;
        let mut column = None;
        let mut rule = None;
        for property in properties.split(',') {
            let Some((key, value)) = property.split_once('=') else {
                continue;
            };
            let value = unescape_annotation(value);
            match key.trim() {
                "file" if !value.is_empty() => path = Some(value),
                "line" => source_line = value.parse::<u64>().ok().filter(|line| *line > 0),
                "col" => column = value.parse::<u64>().ok().filter(|column| *column > 0),
                "title" if !value.is_empty() => rule = Some(value),
                _ => {}
            }
        }
        let message = unescape_annotation(message);
        let message = message.lines().next().unwrap_or_default().trim();
        // Oxlint repeats the location as a `path:line:column: ` message prefix.
        let message = path
            .as_deref()
            .and_then(|path| message.strip_prefix(path))
            .and_then(|rest| rest.strip_prefix(':'))
            .and_then(|rest| rest.split_once(": "))
            .filter(|(location, _)| {
                location.split(':').all(|part| {
                    !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
                })
            })
            .map_or(message, |(_, rest)| rest);
        let message = match rule.as_deref() {
            Some("format") => "File is not formatted; run `quality format` to apply it".to_owned(),
            _ if message.is_empty() => format!("{tool} finding"),
            _ => message.to_owned(),
        };
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: path.map(|path| normalize_path(root, working_directory, &path)),
            line: source_line,
            column: source_line.and(column),
            severity: severity.to_owned(),
            message,
            rule,
        });
    }
    Some(diagnostics)
}

/// Reverses the percent-encoding GitHub requires for annotation properties and messages.
fn unescape_annotation(value: &str) -> String {
    value
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%25", "%")
}

fn parse_eslint_json(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let Ok(files) = serde_json::from_str::<serde_json::Value>(output) else {
        return None;
    };
    let files = files.as_array()?;
    Some(
        files
            .iter()
            .flat_map(|file| {
                let path = file.get("filePath").and_then(|value| value.as_str());
                file.get("messages")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .map(move |message| Diagnostic {
                        tool: tool.to_owned(),
                        path: path.map(|path| normalize_path(root, working_directory, path)),
                        line: message.get("line").and_then(|value| value.as_u64()),
                        column: message.get("column").and_then(|value| value.as_u64()),
                        severity: match message.get("severity").and_then(|value| value.as_u64()) {
                            Some(2) => "error",
                            Some(1) => "warning",
                            _ => "info",
                        }
                        .to_owned(),
                        message: message
                            .get("message")
                            .and_then(|value| value.as_str())
                            .unwrap_or("ESLint finding")
                            .to_owned(),
                        rule: message
                            .get("ruleId")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                    })
            })
            .collect(),
    )
}

fn parse_swiftlint_json(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let Ok(findings) = serde_json::from_str::<serde_json::Value>(output) else {
        return None;
    };
    let findings = findings.as_array()?;
    Some(
        findings
            .iter()
            .map(|finding| Diagnostic {
                tool: tool.to_owned(),
                path: finding
                    .get("file")
                    .and_then(|value| value.as_str())
                    .map(|path| normalize_path(root, working_directory, path)),
                line: finding.get("line").and_then(|value| value.as_u64()),
                column: finding.get("character").and_then(|value| value.as_u64()),
                severity: finding
                    .get("severity")
                    .and_then(|value| value.as_str())
                    .unwrap_or("warning")
                    .to_ascii_lowercase(),
                message: finding
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("SwiftLint finding")
                    .to_owned(),
                rule: finding
                    .get("rule_id")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
            })
            .collect(),
    )
}

fn parse_ktlint_json(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let documents = if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
        match value {
            serde_json::Value::Array(values) => values,
            value => vec![value],
        }
    } else {
        let lines: Vec<_> = output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let parsed: Vec<_> = lines
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if parsed.len() != lines.len() {
            return None;
        }
        parsed
    };
    Some(
        documents
            .iter()
            .flat_map(|file| {
                let path = file.get("file").and_then(|value| value.as_str());
                file.get("errors")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .map(move |error| Diagnostic {
                        tool: tool.to_owned(),
                        path: path.map(|path| normalize_path(root, working_directory, path)),
                        line: error.get("line").and_then(|value| value.as_u64()),
                        column: error.get("column").and_then(|value| value.as_u64()),
                        severity: "warning".to_owned(),
                        message: error
                            .get("message")
                            .and_then(|value| value.as_str())
                            .unwrap_or("ktlint finding")
                            .to_owned(),
                        rule: error
                            .get("rule")
                            .or_else(|| error.get("ruleId"))
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                    })
            })
            .collect(),
    )
}

fn parse_santi_og_json(
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
) -> Option<Vec<Diagnostic>> {
    let result = serde_json::from_str::<serde_json::Value>(output).ok()?;
    if result.get("command").and_then(serde_json::Value::as_str) != Some("check") {
        return None;
    }
    let config = result.get("config").and_then(serde_json::Value::as_str);
    let stale = result.get("stale")?.as_array()?;
    Some(
        stale
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(|output| Diagnostic {
                tool: tool.to_owned(),
                path: config.map(|path| normalize_path(root, working_directory, path)),
                line: None,
                column: None,
                severity: "error".to_owned(),
                message: format!(
                    "Open Graph output `{output}` is missing or stale; run `santi-og generate`."
                ),
                rule: Some("og-output-stale".to_owned()),
            })
            .collect(),
    )
}

fn normalize_path(
    root: &std::path::Path,
    working_directory: &std::path::Path,
    value: &str,
) -> String {
    let path = std::path::Path::new(value);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory.join(path)
    };
    path.strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string()
}

fn classify_failure(output: &str) -> FailureKind {
    if environment_failure_detail(output).is_some() {
        FailureKind::Environment
    } else {
        FailureKind::Code
    }
}

pub fn environment_failure_detail(output: &str) -> Option<&str> {
    const PATTERNS: [&str; 8] = [
        "address already in use",
        "port is already in use",
        "unable to locate a java runtime",
        "could not find java",
        "java_home is not set",
        "no space left on device",
        "too many open files",
        "cannot allocate memory",
    ];
    output.lines().find(|line| {
        let normalized = line.to_ascii_lowercase();
        PATTERNS.iter().any(|pattern| normalized.contains(pattern))
    })
}

fn format_command(invocation: &Invocation) -> String {
    std::iter::once(invocation.command.display().to_string())
        .chain(invocation.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn command_available(root: &std::path::Path, command: Option<&std::path::PathBuf>) -> bool {
    let Some(command) = command else {
        return false;
    };
    if command.components().count() > 1 || command.is_absolute() {
        let path = if command.is_absolute() {
            command.clone()
        } else {
            root.join(command)
        };
        is_executable(&path)
    } else {
        which::which(command).is_ok()
    }
}

fn java_runtime_available(root: &std::path::Path) -> bool {
    Command::new("java")
        .arg("-version")
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "/project";
    const ESLINT_ERROR: &str = r#"[{"filePath":"/project/a.ts","messages":[{"ruleId":"semi","severity":2,"message":"Missing semicolon.","line":1,"column":1}]}]"#;

    fn parse_diagnostics(
        parser: DiagnosticParser,
        tool: &str,
        working_directory: &std::path::Path,
        output: &str,
        synthesize_failure: bool,
    ) -> (Vec<Diagnostic>, bool) {
        let parsed = parse_captured(
            parser,
            tool,
            working_directory,
            output,
            StreamCapture::Complete,
            "",
            synthesize_failure,
        );
        (parsed.diagnostics, parsed.fully_parsed)
    }

    fn parse_streams(
        parser: DiagnosticParser,
        stdout: &str,
        stderr: &str,
        synthesize_failure: bool,
    ) -> ParsedOutput {
        parse_captured(
            parser,
            "tool",
            std::path::Path::new(PROJECT),
            stdout,
            StreamCapture::Complete,
            stderr,
            synthesize_failure,
        )
    }

    fn parse_captured(
        parser: DiagnosticParser,
        tool: &str,
        working_directory: &std::path::Path,
        stdout: &str,
        stdout_capture: StreamCapture,
        stderr: &str,
        synthesize_failure: bool,
    ) -> ParsedOutput {
        let combined = join_streams(stdout.as_bytes(), stderr.as_bytes());
        parse_output(
            parser,
            tool,
            std::path::Path::new(PROJECT),
            working_directory,
            &ToolOutput {
                stdout,
                stdout_capture,
                stderr,
                combined: &combined,
            },
            synthesize_failure,
        )
    }

    #[test]
    fn structured_parsers_read_stdout_without_stderr_warnings() {
        let parsed = parse_streams(
            DiagnosticParser::EslintJson,
            ESLINT_ERROR,
            "(node:7) ExperimentalWarning: noise\n",
            true,
        );
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].path.as_deref(), Some("a.ts"));
        assert!(parsed.fully_parsed);
        assert!(!parsed.unreadable);
    }

    #[test]
    fn failures_without_structured_findings_still_explain_themselves_from_stderr() {
        let parsed = parse_streams(
            DiagnosticParser::EslintJson,
            "[]\n",
            "Error: could not find a configuration file\n",
            true,
        );
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(
            parsed.diagnostics[0].message,
            "Error: could not find a configuration file"
        );
        assert!(!parsed.fully_parsed);

        let silent = parse_streams(DiagnosticParser::Codespell, "", "", true);
        assert!(silent.diagnostics.is_empty());
        assert!(silent.fully_parsed);
    }

    #[test]
    fn malformed_structured_output_is_unreadable_and_not_parsed_as_text() {
        let truncated = &ESLINT_ERROR[..60];
        for synthesize_failure in [false, true] {
            let parsed = parse_streams(
                DiagnosticParser::EslintJson,
                truncated,
                "src/real.ts:3:4: error: from stderr (stderr-rule)\n",
                synthesize_failure,
            );
            assert!(parsed.unreadable);
            assert!(!parsed.fully_parsed);
            let rules = parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.rule.as_deref())
                .collect::<Vec<_>>();
            assert_eq!(
                rules,
                [Some("stderr-rule"), Some(UNREADABLE_OUTPUT_RULE)],
                "stdout fragments must not become diagnostics"
            );
            assert!(parsed.diagnostics[1].message.contains("eslint-json"));
        }
    }

    #[test]
    fn line_oriented_output_from_an_incomplete_capture_is_unreadable() {
        let line = r#"{"type":"typo","path":"src/lib.rs","line_num":1,"byte_offset":0,"typo":"teh","corrections":["the"]}"#;
        let parsed = parse_captured(
            DiagnosticParser::TyposJson,
            "typos",
            std::path::Path::new(PROJECT),
            line,
            StreamCapture::Truncated(line.len()),
            "",
            false,
        );
        assert!(parsed.unreadable);
        assert_eq!(parsed.diagnostics.len(), 2);
        assert!(parsed.diagnostics[1].message.contains("--max-output-bytes"));

        // A closed JSON document proves completeness despite an unfinished pipe.
        let document = parse_captured(
            DiagnosticParser::EslintJson,
            "eslint",
            std::path::Path::new(PROJECT),
            ESLINT_ERROR,
            StreamCapture::Unfinished,
            "",
            false,
        );
        assert!(!document.unreadable);
        assert_eq!(document.diagnostics.len(), 1);
    }

    #[test]
    fn incomplete_blank_structured_streams_never_prove_there_are_no_findings() {
        for parser in [
            DiagnosticParser::Codespell,
            DiagnosticParser::EslintJson,
            DiagnosticParser::SwiftlintJson,
            DiagnosticParser::KtlintJson,
            DiagnosticParser::SantiOgJson,
            DiagnosticParser::TyposJson,
        ] {
            for capture in [StreamCapture::Truncated(3), StreamCapture::Unfinished] {
                let parsed = parse_captured(
                    parser,
                    "tool",
                    std::path::Path::new(PROJECT),
                    " \n ",
                    capture,
                    "",
                    false,
                );
                assert!(parsed.unreadable, "{parser:?} with {capture:?}");
                assert_eq!(
                    parsed.diagnostics.last().unwrap().rule.as_deref(),
                    Some(UNREADABLE_OUTPUT_RULE)
                );
            }
        }
        let generic = parse_captured(
            DiagnosticParser::Generic,
            "tool",
            std::path::Path::new(PROJECT),
            " \n ",
            StreamCapture::Truncated(3),
            "",
            false,
        );
        assert!(!generic.unreadable);
        assert!(generic.diagnostics.is_empty());
    }

    #[test]
    fn json_after_a_script_banner_is_unreadable() {
        let banner = format!("\n> app@1.0.0 lint\n> eslint . --format json\n\n{ESLINT_ERROR}\n");
        for synthesize_failure in [false, true] {
            let parsed = parse_streams(
                DiagnosticParser::EslintJson,
                &banner,
                "",
                synthesize_failure,
            );
            assert!(parsed.unreadable);
            assert_eq!(parsed.diagnostics.len(), 1);
            assert_eq!(
                parsed.diagnostics[0].rule.as_deref(),
                Some(UNREADABLE_OUTPUT_RULE)
            );
        }
    }

    #[test]
    fn typos_binary_file_notices_are_not_findings() {
        let binary = r#"{"type":"binary_file","path":"./assets/logo.png"}"#;
        let typo = r#"{"type":"typo","path":"./src/lib.rs","line_num":3,"byte_offset":4,"typo":"retrive","corrections":["retrieve"]}"#;

        let clean = parse_streams(DiagnosticParser::TyposJson, binary, "", false);
        assert!(clean.diagnostics.is_empty());
        assert!(!clean.unreadable);

        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::TyposJson,
            "typos",
            std::path::Path::new(PROJECT),
            &format!("{binary}\n{typo}\n"),
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some("src/lib.rs"));
        assert!(baseline_safe);
    }

    #[test]
    fn output_in_another_format_keeps_the_generic_fallback() {
        let parsed = parse_streams(
            DiagnosticParser::SwiftlintJson,
            "Sources/App.swift:10:5: warning: Line should be shorter (line_length)\n",
            "",
            false,
        );
        assert!(!parsed.unreadable);
        assert_eq!(
            parsed.diagnostics[0].path.as_deref(),
            Some("Sources/App.swift")
        );
    }

    #[test]
    fn joined_streams_keep_line_boundaries() {
        assert_eq!(
            join_streams(b"checking", b"src/a.ts:1:2: x"),
            "checking\nsrc/a.ts:1:2: x"
        );
        assert_eq!(join_streams(b"done\n", b"warning"), "done\nwarning");
        assert_eq!(join_streams(b"", b"warning"), "warning");
        assert_eq!(join_streams(b"only stdout", b""), "only stdout");

        let (combined, truncated) = combine_limited_output(
            &(b"checking".to_vec(), false),
            &(b"src/a.ts".to_vec(), false),
            16,
        );
        assert_eq!(combined, "checking\nsrc/a.ts");
        assert!(!truncated, "the separator is not captured output");
    }

    #[test]
    fn unparsed_failure_messages_are_bounded_on_character_boundaries() {
        let message = unparsed_failure_message(&format!("\n  {}  \nsecond", "é".repeat(1500)));
        assert_eq!(message.chars().count(), UNPARSED_MESSAGE_LIMIT + 1);
        assert!(message.ends_with('…'));
        assert_eq!(unparsed_failure_message("  short line  "), "short line");
        assert_eq!(
            unparsed_failure_message("\n\n"),
            "tool exited unsuccessfully"
        );
    }

    #[test]
    fn parses_swiftlint_style_diagnostics() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::Generic,
            "swiftlint",
            std::path::Path::new("/project"),
            "Sources/App.swift:10:5: warning: Line should be shorter (line_length)",
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some("Sources/App.swift"));
        assert_eq!(found[0].line, Some(10));
        assert_eq!(found[0].rule.as_deref(), Some("line_length"));
        assert!(baseline_safe);
    }

    #[test]
    fn parses_eslint_json_and_makes_paths_relative() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::EslintJson,
            "eslint",
            std::path::Path::new("/project"),
            r#"[{"filePath":"/project/src/app.ts","messages":[{"ruleId":"semi","severity":2,"message":"Missing semicolon.","line":3,"column":8}]}]"#,
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some("src/app.ts"));
        assert_eq!(found[0].severity, "error");
        assert_eq!(found[0].rule.as_deref(), Some("semi"));
        assert!(baseline_safe);
    }

    #[test]
    fn parses_biome_github_annotations_with_absolute_paths() {
        let stdout = "::warning title=lint/style/useConst,file=/project/src/app.ts,line=3,endLine=3,col=1,endColumn=4::This let declares a variable that is only assigned once.\n::error title=format,file=/project/src/app.ts,line=1,endLine=1,col=2,endColumn=2::Formatter would have printed the following content:%0A%0A  const a = 1;\n";
        let parsed = parse_streams(
            DiagnosticParser::GithubAnnotations,
            stdout,
            "check ━━━\n\n  × Some errors were emitted while running checks.\n",
            true,
        );
        assert!(parsed.fully_parsed);
        assert_eq!(parsed.diagnostics.len(), 2);
        let lint = &parsed.diagnostics[0];
        assert_eq!(lint.path.as_deref(), Some("src/app.ts"));
        assert_eq!((lint.line, lint.column), (Some(3), Some(1)));
        assert_eq!(lint.severity, "warning");
        assert_eq!(lint.rule.as_deref(), Some("lint/style/useConst"));
        let format = &parsed.diagnostics[1];
        assert_eq!(format.severity, "error");
        assert_eq!(format.rule.as_deref(), Some("format"));
        assert_eq!(
            format.message,
            "File is not formatted; run `quality format` to apply it"
        );
    }

    #[test]
    fn parses_oxlint_annotations_and_drops_the_repeated_location() {
        let stdout = "::warning file=src/a%2Cb.ts,line=1,endLine=1,col=1,endColumn=10,title=eslint(no-debugger)::src/a,b.ts:1:1: `debugger` statement is not allowed\n::notice file=src/c.ts,line=0,col=0,title=oxc(x)::Note%3A 100%25 sure\n::group::noise\n\nFound 1 warning and 0 errors.\nFinished in 8ms on 2 files with 96 rules using 15 threads.\n";
        let parsed = parse_streams(DiagnosticParser::GithubAnnotations, stdout, "", false);
        assert!(parsed.fully_parsed);
        assert_eq!(parsed.diagnostics.len(), 2);
        let first = &parsed.diagnostics[0];
        assert_eq!(first.path.as_deref(), Some("src/a,b.ts"));
        assert_eq!(first.message, "`debugger` statement is not allowed");
        assert_eq!(first.rule.as_deref(), Some("eslint(no-debugger)"));
        let notice = &parsed.diagnostics[1];
        assert_eq!(notice.severity, "info");
        assert_eq!((notice.line, notice.column), (None, None));
        assert_eq!(notice.message, "Note: 100% sure");

        let unlocated = parse_streams(
            DiagnosticParser::GithubAnnotations,
            "::error file=src/a.ts,line=2,title=x::src/a.ts:: kept as written\n",
            "",
            false,
        );
        assert_eq!(
            unlocated.diagnostics[0].message,
            "src/a.ts:: kept as written"
        );
    }

    #[test]
    fn annotation_failures_without_findings_explain_themselves_from_stdout() {
        let parsed = parse_streams(
            DiagnosticParser::GithubAnnotations,
            "Failed to parse oxlint configuration file.\n\n  x key must be a string\n",
            "",
            true,
        );
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(
            parsed.diagnostics[0].message,
            "Failed to parse oxlint configuration file."
        );
        assert_eq!(parsed.diagnostics[0].severity, "error");

        let clean = parse_streams(
            DiagnosticParser::GithubAnnotations,
            "Found 0 warnings and 0 errors.\n",
            "",
            false,
        );
        assert!(clean.diagnostics.is_empty());
        assert!(clean.fully_parsed);
    }

    #[test]
    fn parses_swiftlint_json() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::SwiftlintJson,
            "swiftlint",
            std::path::Path::new("/project"),
            r#"[{"character":2,"file":"/project/App.swift","line":7,"reason":"Too long","rule_id":"line_length","severity":"Warning"}]"#,
            true,
        );
        assert_eq!(found[0].path.as_deref(), Some("App.swift"));
        assert_eq!(found[0].severity, "warning");
        assert!(baseline_safe);
    }

    #[test]
    fn parses_codespell_findings() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::Codespell,
            "codespell",
            std::path::Path::new("/project"),
            "docs/guide.md:7: teh ==> the",
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some("docs/guide.md"));
        assert_eq!(found[0].line, Some(7));
        assert_eq!(found[0].rule.as_deref(), Some("spelling"));
        assert!(found[0].message.contains("suggested: the"));
        assert!(baseline_safe);
    }

    #[test]
    fn parses_typos_json_lines() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::TyposJson,
            "typos",
            std::path::Path::new("/project"),
            r#"{"type":"typo","path":"./src/lib.rs","line_num":3,"byte_offset":4,"typo":"retrive","corrections":["retrieve"]}"#,
            true,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some("src/lib.rs"));
        assert_eq!(found[0].line, Some(3));
        assert_eq!(found[0].column, Some(5));
        assert_eq!(found[0].rule.as_deref(), Some("spelling"));
        assert!(baseline_safe);
    }

    #[test]
    fn parses_santi_og_stale_outputs_against_the_workspace_config() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::SantiOgJson,
            "santi-og@apps/site",
            std::path::Path::new("/project/apps/site"),
            r#"{"command":"check","config":"/project/apps/site/og.config.mjs","stale":["index.webp","docs.webp"]}"#,
            true,
        );

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path.as_deref(), Some("apps/site/og.config.mjs"));
        assert_eq!(found[0].severity, "error");
        assert_eq!(found[0].rule.as_deref(), Some("og-output-stale"));
        assert!(found[0].message.contains("index.webp"));
        assert!(baseline_safe);
    }

    #[test]
    fn generic_findings_use_report_severities_and_one_based_positions() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::Generic,
            "cppcheck",
            std::path::Path::new(PROJECT),
            "src/a.cpp:0:0: style: formatting finding\nsrc/b.cpp:0:4: info: no line\nsrc/c.cpp:3:0: error: no column",
            true,
        );
        assert!(baseline_safe);
        assert_eq!(found[0].severity, "info");
        assert_eq!((found[0].line, found[0].column), (None, None));
        assert_eq!((found[1].line, found[1].column), (None, None));
        assert_eq!((found[2].line, found[2].column), (Some(3), None));
    }

    #[test]
    fn mixed_unstructured_output_is_not_safe_to_baseline() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::Generic,
            "detekt",
            std::path::Path::new("/project"),
            "src/App.kt:3:1: warning: Example (example)\nAnalyzer crashed unexpectedly",
            true,
        );
        assert_eq!(found.len(), 1);
        assert!(!baseline_safe);
    }

    #[test]
    fn ignores_generic_diagnostic_looking_output_from_successful_tasks() {
        let (found, baseline_safe) = parse_diagnostics(
            DiagnosticParser::Generic,
            "repository-check",
            std::path::Path::new("/project"),
            "src/app.ts:4:2: warning: informational output",
            false,
        );
        assert!(found.is_empty());
        assert!(!baseline_safe);
    }

    #[test]
    fn classifies_resource_conflicts_as_environment_failures() {
        assert!(matches!(
            classify_failure("Error: address already in use 127.0.0.1:4321"),
            FailureKind::Environment
        ));
        assert!(matches!(
            classify_failure("src/app.ts:1:1: error: Type mismatch"),
            FailureKind::Code
        ));
    }
}
