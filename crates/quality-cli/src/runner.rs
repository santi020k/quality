use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use wait_timeout::ChildExt;

use crate::changes::ChangeSet;
use crate::cli::{AdapterSelection, Severity};
use crate::config::{Config, DiagnosticParser};
use crate::project::Project;
use crate::tools::{self, Invocation};

type CapturedOutput = (Vec<u8>, bool);
type OutputReader = thread::JoinHandle<io::Result<CapturedOutput>>;

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

#[derive(Clone, Debug, Serialize)]
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
    pub results: Vec<ToolResult>,
    pub summary: RunSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<RunScope>,
    #[serde(default)]
    pub suppressed: usize,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

impl RunReport {
    pub fn new(results: Vec<ToolResult>, scope: Option<RunScope>) -> Self {
        let mut report = Self {
            results,
            summary: RunSummary::default(),
            scope,
            suppressed: 0,
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
                && (result.diagnostics.is_empty()
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
        return Ok(RunReport::new(results, scope));
    }

    let count = invocations.len();
    if count == 0 {
        return Ok(RunReport::new(Vec::new(), scope));
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
    Ok(RunReport::new(results, scope))
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
    let child = Command::new(&invocation.command)
        .args(&invocation.args)
        .envs(&invocation.env)
        .current_dir(&invocation.working_directory)
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(mut child) => {
            let stdout = child.stdout.take().map(|stdout| {
                let limit = settings.max_output_bytes;
                thread::spawn(move || read_limited(stdout, limit))
            });
            let stderr = child.stderr.take().map(|stderr| {
                let limit = settings.max_output_bytes;
                thread::spawn(move || read_limited(stderr, limit))
            });
            let timeout = settings.timeout_seconds.or(invocation.timeout_seconds);
            let (status, timed_out) = match timeout {
                Some(seconds) => match child.wait_timeout(Duration::from_secs(seconds)) {
                    Ok(Some(status)) => (Ok(status), false),
                    Ok(None) => {
                        let _ = child.kill();
                        (child.wait(), true)
                    }
                    Err(error) => (Err(error), false),
                },
                None => (child.wait(), false),
            };
            let stdout = join_output(stdout);
            let stderr = join_output(stderr);
            let (combined, output_truncated) =
                combine_limited_output(stdout, stderr, settings.max_output_bytes);
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
                    output: combined,
                    output_truncated,
                    guidance: None,
                    baseline_safe: false,
                };
            }
            let status = match status {
                Ok(status) => status,
                Err(error) => return execution_error(invocation, command_display, started, error),
            };
            let result_status = if status.success() {
                Status::Passed
            } else {
                Status::Failed
            };
            let (diagnostics, baseline_safe) = parse_diagnostics_at(
                invocation.parser,
                &invocation.id,
                root,
                &invocation.working_directory,
                &combined,
                !status.success(),
            );
            ToolResult {
                tool: invocation.id,
                name: invocation.name,
                status: result_status,
                failure_kind: (!status.success()).then(|| classify_failure(&combined)),
                duration_ms: started.elapsed().as_millis(),
                command: command_display,
                diagnostics,
                output: combined,
                output_truncated,
                guidance: None,
                baseline_safe,
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
    }
}

fn read_limited(mut reader: impl Read, limit: usize) -> io::Result<CapturedOutput> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        let keep = read.min(remaining);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    Ok((retained, truncated))
}

fn join_output(handle: Option<OutputReader>) -> CapturedOutput {
    handle
        .and_then(|handle| handle.join().ok())
        .and_then(Result::ok)
        .unwrap_or_else(|| (Vec::new(), false))
}

fn combine_limited_output(
    stdout: (Vec<u8>, bool),
    stderr: (Vec<u8>, bool),
    limit: usize,
) -> (String, bool) {
    let mut bytes = stdout.0;
    bytes.extend_from_slice(&stderr.0);
    let truncated = stdout.1 || stderr.1 || bytes.len() > limit;
    bytes.truncate(limit);
    let mut output = String::from_utf8_lossy(&bytes).to_string();
    if truncated {
        output.push_str(&format!(
            "\n[quality: output truncated after {limit} bytes]\n"
        ));
    }
    (output, truncated)
}

#[cfg(test)]
fn parse_diagnostics(
    parser: DiagnosticParser,
    tool: &str,
    root: &std::path::Path,
    output: &str,
    synthesize_failure: bool,
) -> (Vec<Diagnostic>, bool) {
    parse_diagnostics_at(parser, tool, root, root, output, synthesize_failure)
}

fn parse_diagnostics_at(
    parser: DiagnosticParser,
    tool: &str,
    root: &std::path::Path,
    working_directory: &std::path::Path,
    output: &str,
    synthesize_failure: bool,
) -> (Vec<Diagnostic>, bool) {
    let structured = match parser {
        DiagnosticParser::Codespell => parse_codespell(tool, root, working_directory, output),
        DiagnosticParser::EslintJson => parse_eslint_json(tool, root, working_directory, output),
        DiagnosticParser::SwiftlintJson => {
            parse_swiftlint_json(tool, root, working_directory, output)
        }
        DiagnosticParser::KtlintJson => parse_ktlint_json(tool, root, working_directory, output),
        DiagnosticParser::TyposJson => parse_typos_json(tool, root, working_directory, output),
        DiagnosticParser::Generic => None,
    };
    if let Some(structured) = structured {
        return (structured, true);
    }
    if parser == DiagnosticParser::Generic && !synthesize_failure {
        return (Vec::new(), false);
    }

    let pattern = diagnostic_pattern();
    let mut diagnostics = Vec::new();
    let meaningful_lines: Vec<_> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    for line in &meaningful_lines {
        let Some(captures) = pattern.captures(line.trim()) else {
            continue;
        };
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: captures
                .name("path")
                .map(|value| normalize_path(root, working_directory, value.as_str())),
            line: captures
                .name("line")
                .and_then(|value| value.as_str().parse().ok()),
            column: captures
                .name("column")
                .and_then(|value| value.as_str().parse().ok()),
            severity: captures
                .name("severity")
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| "warning".to_owned()),
            message: captures
                .name("message")
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| (*line).to_owned()),
            rule: captures.name("rule").map(|value| value.as_str().to_owned()),
        });
    }
    let fully_parsed = !diagnostics.is_empty() && diagnostics.len() == meaningful_lines.len();
    if diagnostics.is_empty() && synthesize_failure {
        diagnostics.push(Diagnostic {
            tool: tool.to_owned(),
            path: None,
            line: None,
            column: None,
            severity: "error".to_owned(),
            message: output
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("tool exited unsuccessfully")
                .trim()
                .to_owned(),
            rule: None,
        });
    }
    (diagnostics, fully_parsed)
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
        if finding.get("type").and_then(serde_json::Value::as_str) != Some("typo") {
            return None;
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
    let normalized = output.to_ascii_lowercase();
    if [
        "address already in use",
        "port is already in use",
        "unable to locate a java runtime",
        "could not find java",
        "java_home is not set",
        "no space left on device",
        "too many open files",
        "cannot allocate memory",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
    {
        FailureKind::Environment
    } else {
        FailureKind::Code
    }
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

#[allow(dead_code)]
fn _duration(duration: Duration) -> u128 {
    duration.as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

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
