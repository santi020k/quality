use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use crate::cli::{OutputFormat, Severity};
use crate::runner::{DoctorReport, RunReport, Status};

pub fn print_run(
    report: &RunReport,
    format: OutputFormat,
    report_level: Severity,
    fail_level: Severity,
) -> Result<()> {
    match format {
        OutputFormat::Pretty => print_pretty_run(report, report_level),
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
        }
        OutputFormat::Json | OutputFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(report)?)
        }
        OutputFormat::Github => print_github_doctor(report),
    }
    Ok(())
}

pub fn write_sarif(report: &RunReport, path: &Path, report_level: Severity) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create report directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_vec_pretty(&to_sarif(report, report_level))?;
    fs::write(path, serialized)
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
            println!("No applicable tools found. Run `quality init` after adding project files.");
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
                println!("  ✗ {:<14} {:.2}s", result.name, seconds);
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
        )
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
