use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use crate::cli::{OutputFormat, Severity};
use crate::runner::{DoctorReport, FailureKind, Operation, RunReport, Status};

const AGENT_DIAGNOSTIC_LIMIT: usize = 50;
const AGENT_TOOL_LIMIT: usize = 50;
const AGENT_OUTPUT_LINE_LIMIT: usize = 8;
const AGENT_TEXT_LIMIT: usize = 500;
const AGENT_SELECTION_LIMIT: usize = 8;
const AGENT_SELECTION_ID_LIMIT: usize = 40;

pub fn print_run(
    report: &RunReport,
    operation: Operation,
    format: OutputFormat,
    report_level: Severity,
    fail_level: Severity,
) -> Result<()> {
    match format {
        OutputFormat::Pretty => print_pretty_run(report, report_level),
        OutputFormat::Agent => print!(
            "{}",
            render_agent_run(report, operation, report_level, fail_level)
        ),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(report)?),
        OutputFormat::Sarif => println!(
            "{}",
            serde_json::to_string_pretty(&to_sarif(report, report_level))?
        ),
        OutputFormat::Github => print_github_run(report, report_level, fail_level)?,
    }
    Ok(())
}

pub fn print_doctor(report: &DoctorReport, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Pretty => {
            println!("Project: {}", report.root);
            println!("Config:  {}", report.config);
            println!();
            for entry in &report.tools {
                if !entry.enabled {
                    let reason = if entry.detected {
                        "disabled"
                    } else {
                        "not detected"
                    };
                    println!("  – {:<14} {reason}", entry.name);
                } else if !entry.check_enabled {
                    let reason = if entry.format_or_fix_available {
                        "check disabled; format/fix available"
                    } else {
                        "check disabled"
                    };
                    println!("  – {:<14} {reason}", entry.name);
                } else if entry.available {
                    println!("  ✓ {:<14} {}", entry.name, entry.command);
                } else if entry.required {
                    println!("  ✗ {:<14} missing", entry.name);
                    if let Some(guidance) = &entry.guidance {
                        println!("    {guidance}");
                    }
                } else {
                    println!("  ! {:<14} missing (optional)", entry.name);
                }
            }
            if let Some(preset) = &report.preset {
                println!();
                match preset.state.as_str() {
                    "current" => println!(
                        "Preset:  {} (catalog {}, current)",
                        preset.profile, preset.catalog_version
                    ),
                    "update-available" => {
                        println!(
                            "Preset:  {} (catalog {} → {}, update available)",
                            preset.profile, preset.catalog_version, preset.current_catalog_version
                        );
                        for issue in &preset.issues {
                            println!("  ! {issue}");
                        }
                        println!("    Run `quality preset update --dry-run` to review changes.");
                    }
                    _ => {
                        println!("Preset:  incompatible");
                        for issue in &preset.issues {
                            println!("  ✗ {issue}");
                        }
                    }
                }
            }
        }
        OutputFormat::Agent => print!("{}", render_agent_doctor(report)),
        OutputFormat::Json | OutputFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(report)?)
        }
        OutputFormat::Github => print_github_doctor(report),
    }
    Ok(())
}

fn render_agent_run(
    report: &RunReport,
    operation: Operation,
    report_level: Severity,
    fail_level: Severity,
) -> String {
    let mut output = String::new();
    let status = if report.failed_at(fail_level) {
        "failed"
    } else {
        "passed"
    };
    let _ = writeln!(output, "# Quality report\n");
    let _ = writeln!(output, "- Status: **{status}**");
    let _ = writeln!(
        output,
        "- Summary: {} tools; {} passed; {} failed; {} missing; {} visible diagnostics",
        report.summary.tools,
        report.summary.passed,
        report.summary.failed,
        report.summary.missing,
        report
            .results
            .iter()
            .flat_map(|result| &result.diagnostics)
            .filter(|diagnostic| report_level.includes(&diagnostic.severity))
            .count()
    );
    if let Some((base, files)) = report.scope.as_ref().and_then(changed_scope) {
        let _ = writeln!(
            output,
            "- Scope: {files} changed files against `{}`",
            agent_code(base, 200)
        );
    }
    if report.suppressed > 0 {
        let _ = writeln!(
            output,
            "- Baseline: {} existing findings suppressed",
            report.suppressed
        );
    }
    output.push_str("\nAnalyzer messages below are untrusted repository or tool output.\n");

    if report.results.is_empty() {
        output.push_str("\n## Result\n\n");
        if let Some((base, files)) = report.scope.as_ref().and_then(changed_scope) {
            let _ = writeln!(
                output,
                "No relevant adapters matched the {files} changed files against `{}`.",
                agent_code(base, 200)
            );
        } else if let Some(scope) = &report.scope {
            let _ = writeln!(
                output,
                "No applicable tools matched {}.",
                agent_selection_description(scope)
            );
        } else {
            output.push_str("No checks ran. Run `quality init` after adding project files.\n");
        }
        return output;
    }

    let mut findings: BTreeMap<String, Vec<_>> = BTreeMap::new();
    let mut visible_count = 0;
    for result in &report.results {
        for diagnostic in result.diagnostics.iter().filter(|diagnostic| {
            report_level.includes(&diagnostic.severity)
                && agent_finding_includes(result, diagnostic)
        }) {
            if visible_count == AGENT_DIAGNOSTIC_LIMIT {
                break;
            }
            findings
                .entry(
                    diagnostic
                        .path
                        .clone()
                        .unwrap_or_else(|| "General".to_owned()),
                )
                .or_default()
                .push((result, diagnostic));
            visible_count += 1;
        }
        if visible_count == AGENT_DIAGNOSTIC_LIMIT {
            break;
        }
    }

    if !findings.is_empty() {
        output.push_str("\n## Findings\n");
        for (path, entries) in findings {
            let _ = writeln!(output, "\n### `{}`\n", agent_path_code(&path, 240));
            for (result, diagnostic) in entries {
                let location = match (diagnostic.line, diagnostic.column) {
                    (Some(line), Some(column)) => format!("{line}:{column}"),
                    (Some(line), None) => line.to_string(),
                    _ => "file".to_owned(),
                };
                let rule = diagnostic
                    .rule
                    .as_deref()
                    .map(|rule| format!(" `{}`", agent_code(rule, 120)))
                    .unwrap_or_default();
                let _ = writeln!(
                    output,
                    "- `{location}` **{}**{rule}: {} _(via {})_",
                    agent_text(&diagnostic.severity, 24),
                    agent_text(&diagnostic.message, AGENT_TEXT_LIMIT),
                    agent_text(&result.name, 120)
                );
            }
        }
        let total_visible = report
            .results
            .iter()
            .flat_map(|result| {
                result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| agent_finding_includes(result, diagnostic))
            })
            .filter(|diagnostic| report_level.includes(&diagnostic.severity))
            .count();
        if total_visible > visible_count {
            let _ = writeln!(
                output,
                "\n_{} additional diagnostics omitted; use `--format json` for the complete report._",
                total_visible - visible_count
            );
        }
    }

    let execution_problems = report
        .results
        .iter()
        .filter(|result| {
            matches!(
                result.failure_kind,
                Some(FailureKind::Environment | FailureKind::Toolchain)
            )
        })
        .collect::<Vec<_>>();
    let mut remaining_tool_entries = AGENT_TOOL_LIMIT;
    let mut shown_tool_entries = execution_problems.len().min(remaining_tool_entries);
    if shown_tool_entries > 0 {
        output.push_str("\n## Environment and toolchain problems\n\n");
        for result in execution_problems.iter().take(shown_tool_entries) {
            let category = match result.failure_kind {
                Some(FailureKind::Environment) => "environment",
                Some(FailureKind::Toolchain) => "toolchain",
                Some(FailureKind::Code) | None => "code",
            };
            let detail = result
                .guidance
                .as_deref()
                .or_else(|| {
                    matches!(result.failure_kind, Some(FailureKind::Environment))
                        .then(|| crate::runner::environment_failure_detail(&result.output))
                        .flatten()
                })
                .or_else(|| {
                    result
                        .diagnostics
                        .iter()
                        .find(|item| item.path.is_none())
                        .map(|item| item.message.as_str())
                });
            let detail = match (&result.status, detail) {
                (_, Some(detail)) => detail,
                (&Status::Missing, None) => {
                    "Optional tool is unavailable; install it or run `quality doctor --format agent` for setup guidance."
                }
                (&Status::Failed | &Status::Passed, None) => {
                    "Adapter output indicates an execution failure; inspect the complete JSON report."
                }
            };
            let _ = writeln!(
                output,
                "- **{}** ({category}): {}",
                agent_text(&result.name, 120),
                agent_text(detail, AGENT_TEXT_LIMIT)
            );
        }
    }
    remaining_tool_entries -= shown_tool_entries;

    let raw_failures = report
        .results
        .iter()
        .filter(|result| {
            matches!(result.status, Status::Failed)
                && !matches!(
                    result.failure_kind,
                    Some(FailureKind::Environment | FailureKind::Toolchain)
                )
                && (result.diagnostics.is_empty() || agent_has_synthesized_failure(result))
        })
        .collect::<Vec<_>>();
    let shown_raw_failures = raw_failures.len().min(remaining_tool_entries);
    shown_tool_entries += shown_raw_failures;
    remaining_tool_entries -= shown_raw_failures;
    if shown_raw_failures > 0 {
        output.push_str("\n## Unstructured failure output\n");
        for result in raw_failures.iter().take(shown_raw_failures) {
            let _ = writeln!(output, "\n### {}\n", agent_text(&result.name, 120));
            for line in result.output.lines().take(AGENT_OUTPUT_LINE_LIMIT) {
                let _ = writeln!(output, "    {}", agent_text(line, 240));
            }
            if result.output.lines().count() > AGENT_OUTPUT_LINE_LIMIT || result.output_truncated {
                output.push_str("    [additional output omitted]\n");
            }
        }
    }

    let mut rerun_adapters = BTreeSet::new();
    for result in &report.results {
        if !matches!(result.status, Status::Passed) {
            rerun_adapters.insert(result.tool.split('@').next().unwrap_or(&result.tool));
        }
    }
    let shown_reruns = rerun_adapters.len().min(remaining_tool_entries);
    if shown_reruns > 0 {
        output.push_str("\n## Focused reruns\n\n");
        for adapter in rerun_adapters.iter().take(shown_reruns) {
            let _ = writeln!(
                output,
                "- `quality {} --only {}`",
                operation_command(operation),
                agent_code(adapter, usize::MAX)
            );
        }
        shown_tool_entries += shown_reruns;
    }
    let total_tool_entries = execution_problems.len() + raw_failures.len() + rerun_adapters.len();
    if total_tool_entries > shown_tool_entries {
        let _ = writeln!(
            output,
            "\n_{} additional tool entries omitted; use `--format json` for the complete report._",
            total_tool_entries - shown_tool_entries
        );
    }
    output
}

fn render_agent_doctor(report: &DoctorReport) -> String {
    let mut output = String::new();
    let status = if report.has_errors() {
        "blocked"
    } else {
        "ready"
    };
    let available = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && entry.available)
        .count();
    let required_missing = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && entry.required && !entry.available)
        .count();
    let optional_missing = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && !entry.required && !entry.available)
        .count();
    let _ = writeln!(output, "# Quality doctor\n");
    let _ = writeln!(output, "- Status: **{status}**");
    let _ = writeln!(
        output,
        "- Project: `{}`",
        agent_path_code(&report.root, 240)
    );
    let _ = writeln!(output, "- Config: {}", agent_text(&report.config, 240));
    let _ = writeln!(
        output,
        "- Summary: {available} available; {required_missing} required missing; {optional_missing} optional missing"
    );

    let missing_total = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && !entry.available)
        .count();
    let actionable = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && !entry.available)
        .take(AGENT_TOOL_LIMIT)
        .collect::<Vec<_>>();
    let mut shown_tools = actionable.len();
    if !actionable.is_empty() {
        output.push_str("\n## Missing tools\n\n");
        for entry in actionable {
            let requirement = if entry.required {
                "required"
            } else {
                "optional"
            };
            let guidance = entry
                .guidance
                .as_deref()
                .unwrap_or("Install or configure this tool.");
            let _ = writeln!(
                output,
                "- **{}** (`{}`, {requirement}): {}",
                agent_text(&entry.name, 120),
                agent_code(&entry.tool, 120),
                agent_text(guidance, AGENT_TEXT_LIMIT)
            );
        }
    }

    let configured_total = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && entry.available)
        .count();
    let configured = report
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && entry.available)
        .take(AGENT_TOOL_LIMIT - shown_tools)
        .collect::<Vec<_>>();
    shown_tools += configured.len();
    if !configured.is_empty() {
        output.push_str("\n## Available checks\n\n");
        for entry in configured {
            let _ = writeln!(
                output,
                "- **{}** (`{}`): `{}`",
                agent_text(&entry.name, 120),
                agent_code(&entry.tool, 120),
                agent_code(&entry.command, 240)
            );
        }
    }
    let omitted_tools = missing_total + configured_total - shown_tools;
    if omitted_tools > 0 {
        let _ = writeln!(
            output,
            "\n_{omitted_tools} additional tool entries omitted; use `--format json` for the complete report._"
        );
    }

    if let Some(preset) = &report.preset {
        let _ = writeln!(
            output,
            "\n## Preset\n\n- `{}` catalog {}: **{}**",
            agent_code(&preset.profile, 80),
            preset.catalog_version,
            agent_text(&preset.state, 80)
        );
        for issue in preset.issues.iter().take(AGENT_TOOL_LIMIT) {
            let _ = writeln!(output, "- {}", agent_text(issue, AGENT_TEXT_LIMIT));
        }
    }

    output.push_str("\n## Next command\n\n");
    if report.has_errors() {
        output.push_str("Install or configure the required tools, then run `quality doctor --format agent` again.\n");
    } else {
        output.push_str("Run `quality check --format agent`.\n");
    }
    output
}

fn operation_command(operation: Operation) -> &'static str {
    match operation {
        Operation::Check => "check",
        Operation::CheckFormat => "format --check",
        Operation::Format => "format",
        Operation::Fix => "fix",
    }
}

fn agent_text(value: &str, limit: usize) -> String {
    let normalized = normalized_agent_text(value, limit);
    normalized
        .replace('\\', "\\\\")
        .replace('<', "&lt;")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('`', "\\`")
}

fn normalized_agent_text(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut shortened = normalized.chars().take(limit).collect::<String>();
    if normalized.chars().count() > limit {
        shortened.push('…');
    }
    shortened
}

fn agent_code(value: &str, limit: usize) -> String {
    normalized_agent_text(value, limit).replace('`', "'")
}

fn agent_path_code(value: &str, limit: usize) -> String {
    let significant_whitespace = value.starts_with(' ')
        || value.ends_with(' ')
        || value.contains("  ")
        || value.contains(['\t', '\n', '\r']);
    let mut output = String::new();
    let mut chars = value.chars();
    for character in chars.by_ref().take(limit) {
        match character {
            ' ' if significant_whitespace => output.push_str("\\x20"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '`' => output.push_str("\\x60"),
            _ => output.push(character),
        }
    }
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

fn agent_finding_includes(
    result: &crate::runner::ToolResult,
    diagnostic: &crate::runner::Diagnostic,
) -> bool {
    !agent_has_synthesized_failure(result)
        && (diagnostic.path.is_some()
            || !matches!(
                result.failure_kind,
                Some(FailureKind::Environment | FailureKind::Toolchain)
            ))
}

fn agent_has_synthesized_failure(result: &crate::runner::ToolResult) -> bool {
    if !matches!(result.failure_kind, Some(FailureKind::Code) | None)
        || result.diagnostics.len() != 1
    {
        return false;
    }
    let diagnostic = &result.diagnostics[0];
    diagnostic.path.is_none()
        && diagnostic.line.is_none()
        && diagnostic.column.is_none()
        && diagnostic.rule.is_none()
        && result
            .output
            .lines()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim() == diagnostic.message)
}

fn agent_selection_description(scope: &crate::runner::RunScope) -> String {
    let mut remaining = AGENT_SELECTION_LIMIT;
    let mut included = 0;
    let mut parts = Vec::new();
    for (label, values) in [("only", &scope.only), ("excluding", &scope.exclude)] {
        let selected = values
            .iter()
            .take(remaining)
            .map(|value| agent_code(value, AGENT_SELECTION_ID_LIMIT))
            .collect::<Vec<_>>();
        if !selected.is_empty() {
            included += selected.len();
            remaining -= selected.len();
            parts.push(format!("{label} {}", selected.join(", ")));
        }
    }
    let omitted = scope.only.len() + scope.exclude.len() - included;
    if omitted > 0 {
        parts.push(format!("{omitted} selections omitted"));
    }
    parts.join("; ")
}

pub fn write_sarif(report: &RunReport, path: &Path, report_level: Severity) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create report directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_vec_pretty(&to_sarif(report, report_level))?;
    crate::atomic::write(path, &serialized)
        .with_context(|| format!("could not write SARIF report to {}", path.display()))?;
    Ok(())
}

fn print_github_run(
    report: &RunReport,
    report_level: Severity,
    fail_level: Severity,
) -> Result<()> {
    let mut annotations = 0;
    for result in &report.results {
        for diagnostic in &result.diagnostics {
            if !report_level.includes(&diagnostic.severity) {
                continue;
            }
            annotations += 1;
            let command = github_level(&diagnostic.severity);
            let mut properties = Vec::new();
            if let Some(path) = &diagnostic.path {
                properties.push(format!("file={}", escape_github_property(path)));
            }
            if let Some(line) = diagnostic.line {
                properties.push(format!("line={line}"));
            }
            if let Some(column) = diagnostic.column {
                properties.push(format!("col={column}"));
            }
            let title = diagnostic
                .rule
                .as_ref()
                .map(|rule| format!("{} ({rule})", result.name))
                .unwrap_or_else(|| result.name.clone());
            properties.push(format!("title={}", escape_github_property(&title)));
            println!(
                "::{command} {}::{}",
                properties.join(","),
                escape_github_data(&diagnostic.message)
            );
        }
    }

    let passed = report
        .results
        .iter()
        .filter(|result| matches!(result.status, Status::Passed))
        .count();
    if report.failed_at(fail_level) {
        println!(
            "::notice title=quality::{} annotations from {} tools; {} passed",
            annotations,
            report.results.len(),
            passed
        );
    } else {
        let detail = report
            .scope
            .as_ref()
            .and_then(changed_scope)
            .map(|(_, files)| format!(" for {files} changed files"))
            .unwrap_or_default();
        println!(
            "::notice title=quality::All {} quality tools passed{}",
            report.results.len(),
            detail
        );
    }
    if report.suppressed > 0 {
        println!(
            "::notice title=quality baseline::{} existing findings suppressed",
            report.suppressed
        );
    }
    write_github_summary(report, report_level, fail_level)?;
    Ok(())
}

fn write_github_summary(
    report: &RunReport,
    report_level: Severity,
    fail_level: Severity,
) -> Result<()> {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("could not open GitHub step summary {path}"))?;
    writeln!(file, "## Quality report\n")?;
    if let Some((base, files)) = report.scope.as_ref().and_then(changed_scope) {
        writeln!(
            file,
            "Checked **{} changed files** against `{}`.\n",
            files, base
        )?;
    }
    if let Some(scope) = &report.scope {
        write_selection_summary(&mut file, scope)?;
    }
    writeln!(file, "| Adapter | Result | Findings | Duration |")?;
    writeln!(file, "| --- | --- | ---: | ---: |")?;
    for result in &report.results {
        let findings = result
            .diagnostics
            .iter()
            .filter(|diagnostic| report_level.includes(&diagnostic.severity))
            .count();
        let status = if RunReport::result_failed_at(result, fail_level) {
            "❌ Failed"
        } else {
            match result.status {
                Status::Passed => "✅ Passed",
                Status::Failed => "⚠️ Findings",
                Status::Missing => "➖ Optional",
            }
        };
        writeln!(
            file,
            "| {} | {status} | {findings} | {:.2}s |",
            escape_markdown_cell(&result.name),
            result.duration_ms as f64 / 1000.0
        )?;
    }
    if report.suppressed > 0 {
        writeln!(
            file,
            "\n{} baseline findings were suppressed.",
            report.suppressed
        )?;
    }
    Ok(())
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn write_selection_summary(file: &mut impl Write, scope: &crate::runner::RunScope) -> Result<()> {
    if !scope.only.is_empty() {
        writeln!(file, "Selected adapters: `{}`.\n", scope.only.join("`, `"))?;
    }
    if !scope.exclude.is_empty() {
        writeln!(
            file,
            "Excluded adapters: `{}`.\n",
            scope.exclude.join("`, `")
        )?;
    }
    Ok(())
}

fn print_github_doctor(report: &DoctorReport) {
    let mut missing = 0;
    for entry in &report.tools {
        if entry.enabled && entry.required && !entry.available {
            missing += 1;
            let message = entry
                .guidance
                .as_deref()
                .unwrap_or("Required tool is missing");
            println!(
                "::error title={}::{}",
                escape_github_property(&format!("{} is missing", entry.name)),
                escape_github_data(message)
            );
        }
    }
    if missing == 0 {
        println!("::notice title=quality doctor::All required tools are available");
    }
}

fn github_level(severity: &str) -> &str {
    match severity {
        "error" => "error",
        "warning" => "warning",
        _ => "notice",
    }
}

fn escape_github_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_github_property(value: &str) -> String {
    escape_github_data(value)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn print_pretty_run(report: &RunReport, report_level: Severity) {
    if report.results.is_empty() {
        if let Some((base, files)) = report.scope.as_ref().and_then(changed_scope) {
            println!(
                "No relevant changed files found against {} ({} changed files inspected).",
                base, files
            );
        } else if let Some(scope) = &report.scope {
            println!(
                "No applicable tools matched {}.",
                selection_description(scope)
            );
        } else {
            println!("Warning: no checks ran. Run `quality init` after adding project files.");
        }
        return;
    }
    if let Some((base, files)) = report.scope.as_ref().and_then(changed_scope) {
        println!("Changed files: {files} against {base}");
    }
    if let Some(scope) = &report.scope {
        if !scope.only.is_empty() || !scope.exclude.is_empty() {
            println!("Adapters: {}", selection_description(scope));
        }
        if scope.mode.is_some() || !scope.only.is_empty() || !scope.exclude.is_empty() {
            println!();
        }
    }
    for result in &report.results {
        let seconds = result.duration_ms as f64 / 1000.0;
        match result.status {
            Status::Passed => println!("  ✓ {:<14} {:.2}s", result.name, seconds),
            Status::Missing => {
                if result.guidance.is_some() {
                    println!("  ✗ {:<14} missing", result.name);
                } else {
                    println!("  – {:<14} missing (optional)", result.name);
                }
                if let Some(guidance) = &result.guidance {
                    println!("    {guidance}");
                }
            }
            Status::Failed => {
                let category = match result.failure_kind {
                    Some(FailureKind::Environment) => " (environment)",
                    Some(FailureKind::Toolchain) => " (toolchain)",
                    Some(FailureKind::Code) | None => "",
                };
                println!("  ✗ {:<14} {:.2}s{category}", result.name, seconds);
                let visible: Vec<_> = result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| report_level.includes(&diagnostic.severity))
                    .collect();
                for diagnostic in visible.iter().take(20) {
                    let location = match (&diagnostic.path, diagnostic.line, diagnostic.column) {
                        (Some(path), Some(line), Some(column)) => format!("{path}:{line}:{column}"),
                        (Some(path), Some(line), None) => format!("{path}:{line}"),
                        (Some(path), None, None) => path.clone(),
                        _ => result.tool.clone(),
                    };
                    let rule = diagnostic
                        .rule
                        .as_ref()
                        .map(|rule| format!(" [{rule}]"))
                        .unwrap_or_default();
                    println!(
                        "    {location} {}: {}{rule}",
                        diagnostic.severity, diagnostic.message
                    );
                }
                if visible.len() > 20 {
                    println!("    … and {} more", visible.len() - 20);
                }
                if result.diagnostics.len() == 1 && result.diagnostics[0].path.is_none() {
                    for line in result.output.lines().skip(1).take(8) {
                        println!("    {line}");
                    }
                }
            }
        }
    }

    if report.suppressed > 0 {
        println!(
            "  – Baseline       {} existing findings hidden",
            report.suppressed
        );
    }

    let passed = report
        .results
        .iter()
        .filter(|result| matches!(result.status, Status::Passed))
        .count();
    let optional = report
        .results
        .iter()
        .filter(|result| matches!(result.status, Status::Missing) && result.guidance.is_none())
        .count();
    let failed = report.results.len() - passed - optional;
    println!();
    if failed == 0 {
        if optional == 0 {
            println!("Quality checks passed ({passed} tools).")
        } else {
            println!("Quality checks passed ({passed} tools, {optional} optional unavailable).")
        }
    } else {
        println!(
            "Quality checks found problems ({failed} of {} tools).",
            report.results.len()
        );
        if report.summary.diagnostics > 0 {
            let rules = report
                .summary
                .rules
                .iter()
                .map(|(rule, count)| format!("{rule} ({count})"))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "Diagnostics: {} errors, {} warnings, {} info across {} files{}",
                report.summary.errors,
                report.summary.warnings,
                report.summary.info,
                report.summary.files.len(),
                if rules.is_empty() {
                    String::new()
                } else {
                    format!("; rules: {rules}")
                }
            );
        }
    }
}

fn to_sarif(report: &RunReport, report_level: Severity) -> serde_json::Value {
    let runs = report
        .results
        .iter()
        .map(|tool_result| {
            let results = tool_result
                .diagnostics
                .iter()
                .filter(|diagnostic| report_level.includes(&diagnostic.severity))
                .map(|diagnostic| {
                    let mut result = json!({
                        "level": sarif_level(&diagnostic.severity),
                        "message": { "text": diagnostic.message }
                    });
                    if let Some(rule) = &diagnostic.rule {
                        result["ruleId"] = json!(rule);
                    }
                    if let Some(path) = &diagnostic.path {
                        let mut region = json!({});
                        if let Some(line) = diagnostic.line {
                            region["startLine"] = json!(line);
                        }
                        if let Some(column) = diagnostic.column {
                            region["startColumn"] = json!(column);
                        }
                        result["locations"] = json!([{
                            "physicalLocation": {
                                "artifactLocation": { "uri": path },
                                "region": region
                            }
                        }]);
                    }
                    result
                })
                .collect::<Vec<_>>();
            let mut run = json!({
                "tool": {
                    "driver": {
                        "name": tool_result.name
                    }
                },
                "invocations": [{
                    "executionSuccessful": matches!(tool_result.status, Status::Passed),
                    "commandLine": tool_result.command
                }],
                "results": results
            });
            if let Some(scope) = &report.scope {
                run["properties"] = json!({ "qualityScope": scope });
            }
            run
        })
        .collect::<Vec<_>>();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": runs
    })
}

fn changed_scope(scope: &crate::runner::RunScope) -> Option<(&str, usize)> {
    Some((scope.base.as_deref()?, scope.files?))
}

fn selection_description(scope: &crate::runner::RunScope) -> String {
    let mut parts = Vec::new();
    if !scope.only.is_empty() {
        parts.push(format!("only {}", scope.only.join(", ")));
    }
    if !scope.exclude.is_empty() {
        parts.push(format!("excluding {}", scope.exclude.join(", ")));
    }
    parts.join("; ")
}

fn sarif_level(severity: &str) -> &str {
    match severity {
        "error" => "error",
        "warning" => "warning",
        _ => "note",
    }
}
