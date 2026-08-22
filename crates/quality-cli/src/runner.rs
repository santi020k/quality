use std::io;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;

use crate::changes::ChangeSet;
use crate::cli::{AdapterSelection, Severity};
use crate::config::{Config, DiagnosticParser};
use crate::project::Project;
use crate::tools::{self, Invocation};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(skip)]
    pub baseline_safe: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    pub results: Vec<ToolResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<RunScope>,
    #[serde(default)]
    pub suppressed: usize,
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
    }
}

pub fn execute(
    project: &Project,
    config: &Config,
    operation: Operation,
    fail_fast: bool,
    changes: Option<&ChangeSet>,
    selection: &AdapterSelection,
) -> Result<RunReport> {
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
            let result = run_one(&project.root, invocation);
            let failed = matches!(result.status, Status::Failed | Status::Missing);
            results.push(result);
            if failed {
                break;
            }
        }
        return Ok(RunReport {
            results,
            scope,
            suppressed: 0,
        });
    }

    let (sender, receiver) = mpsc::channel();
    let count = invocations.len();
    for invocation in invocations {
        let sender = sender.clone();
        let root = project.root.clone();
        thread::spawn(move || {
            let _ = sender.send(run_one(&root, invocation));
        });
    }
    drop(sender);
    let mut results: Vec<_> = receiver.iter().take(count).collect();
    results.sort_by(|left, right| left.tool.cmp(&right.tool));
    Ok(RunReport {
        results,
        scope,
        suppressed: 0,
    })
}

fn run_one(root: &std::path::Path, invocation: Invocation) -> ToolResult {
    let started = Instant::now();
    let command_display = format_command(&invocation);
    let output = Command::new(&invocation.command)
        .args(&invocation.args)
        .envs(&invocation.env)
        .current_dir(&invocation.working_directory)
        .env("NO_COLOR", "1")
        .output();

    match output {
        Ok(output) => {
            let combined = combine_output(&output.stdout, &output.stderr);
            let status = if output.status.success() {
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
                !output.status.success(),
            );
            ToolResult {
                tool: invocation.id,
                name: invocation.name,
                status,
                failure_kind: (!output.status.success()).then(|| classify_failure(&combined)),
                duration_ms: started.elapsed().as_millis(),
                command: command_display,
                diagnostics,
                output: combined,
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
            guidance: None,
            baseline_safe: false,
        },
    }
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
        DiagnosticParser::EslintJson => parse_eslint_json(tool, root, working_directory, output),
        DiagnosticParser::SwiftlintJson => {
            parse_swiftlint_json(tool, root, working_directory, output)
        }
        DiagnosticParser::KtlintJson => parse_ktlint_json(tool, root, working_directory, output),
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

fn combine_output(stdout: &[u8], stderr: &[u8]) -> String {
    [
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr),
    ]
    .into_iter()
    .filter(|value| !value.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n")
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
