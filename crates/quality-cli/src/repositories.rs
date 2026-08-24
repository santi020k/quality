use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::{AdoptionFormat, AuditFailureCondition, RepositoriesCommand};
use crate::config::{self, Config};
use crate::project::Project;
use crate::{runner, tools};

#[derive(Debug, Serialize)]
struct AdoptionReport {
    schema_version: u8,
    parent: String,
    repositories: Vec<RepositoryEntry>,
    summary: AdoptionSummary,
}

#[derive(Debug, Serialize)]
struct RepositoryEntry {
    path: String,
    configured: bool,
    created: bool,
    status: &'static str,
    detected_adapters: Vec<String>,
    generated_tasks: Vec<String>,
    doctor_errors: usize,
    missing_toolchains: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct AdoptionSummary {
    total: usize,
    configured: usize,
    needs_configuration: usize,
    invalid: usize,
    missing_toolchains: usize,
    created: usize,
}

pub fn run(parent: &Path, command: RepositoriesCommand) -> Result<()> {
    let (apply, dry_run, format, fail_on) = match command {
        RepositoriesCommand::Audit { format, fail_on } => (false, true, format, fail_on),
        RepositoriesCommand::Apply { dry_run, format } => (true, dry_run, format, Vec::new()),
    };
    let report = inspect(parent, apply, dry_run)?;
    print_report(&report, format)?;
    if audit_failed(&report, &fail_on) {
        std::process::exit(1);
    }
    Ok(())
}

fn audit_failed(report: &AdoptionReport, fail_on: &[AuditFailureCondition]) -> bool {
    fail_on.iter().any(|condition| match condition {
        AuditFailureCondition::Invalid => report.summary.invalid > 0,
        AuditFailureCondition::MissingConfiguration => report.summary.needs_configuration > 0,
        AuditFailureCondition::MissingToolchain => report.summary.missing_toolchains > 0,
    })
}

fn inspect(parent: &Path, apply: bool, dry_run: bool) -> Result<AdoptionReport> {
    let repositories = repository_roots(parent)?;
    let mut entries = Vec::with_capacity(repositories.len());
    for root in repositories {
        entries.push(inspect_one(parent, &root, apply, dry_run));
    }
    let summary = AdoptionSummary {
        total: entries.len(),
        configured: entries.iter().filter(|entry| entry.configured).count(),
        needs_configuration: entries
            .iter()
            .filter(|entry| entry.status == "needs_configuration")
            .count(),
        invalid: entries
            .iter()
            .filter(|entry| entry.status == "invalid")
            .count(),
        missing_toolchains: entries
            .iter()
            .filter(|entry| entry.status == "missing_toolchains")
            .count(),
        created: entries.iter().filter(|entry| entry.created).count(),
    };
    Ok(AdoptionReport {
        schema_version: runner::REPORT_SCHEMA_VERSION,
        parent: parent.display().to_string(),
        repositories: entries,
        summary,
    })
}

fn inspect_one(parent: &Path, root: &Path, apply: bool, dry_run: bool) -> RepositoryEntry {
    let relative = root.strip_prefix(parent).unwrap_or(root);
    let path = if relative.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative.display().to_string()
    };
    let project = match Project::discover(root) {
        Ok(project) => project,
        Err(error) => return invalid_entry(path, error.to_string()),
    };
    let detected_adapters = tools::catalog()
        .into_iter()
        .filter(|tool| tool.detect(&project))
        .map(|tool| tool.id.to_owned())
        .collect();
    let generated = config::initial_text(&project)
        .ok()
        .and_then(|text| serde_yaml::from_str::<Config>(&text).ok());
    let generated_tasks = generated
        .as_ref()
        .map(|config| config.tasks.keys().cloned().collect())
        .unwrap_or_default();
    let config_path = root.join("quality.yml");
    let initially_configured = config_path.exists();
    let mut created = false;
    if apply && !initially_configured && !dry_run {
        if let Err(error) = config::write_initial(&config_path, &project, false) {
            return invalid_entry(path, error.to_string());
        }
        created = true;
    }
    let configured = initially_configured || created;
    if !configured {
        return RepositoryEntry {
            path,
            configured: false,
            created: false,
            status: "needs_configuration",
            detected_adapters,
            generated_tasks,
            doctor_errors: 0,
            missing_toolchains: Vec::new(),
            error: None,
        };
    }
    let config = match Config::load_or_default(root) {
        Ok(config) => config,
        Err(error) => {
            return invalid_entry_with_detection(
                path,
                created,
                detected_adapters,
                generated_tasks,
                error.to_string(),
            );
        }
    };
    let doctor = runner::doctor(&project, &config);
    let missing_toolchains = doctor
        .tools
        .iter()
        .filter(|entry| entry.check_enabled && entry.required && !entry.available)
        .map(|entry| entry.tool.clone())
        .collect::<Vec<_>>();
    let doctor_errors = missing_toolchains.len();
    RepositoryEntry {
        path,
        configured: true,
        created,
        status: if doctor_errors == 0 {
            "ready"
        } else {
            "missing_toolchains"
        },
        detected_adapters,
        generated_tasks,
        doctor_errors,
        missing_toolchains,
        error: None,
    }
}

fn invalid_entry(path: String, error: String) -> RepositoryEntry {
    invalid_entry_with_detection(path, false, Vec::new(), Vec::new(), error)
}

fn invalid_entry_with_detection(
    path: String,
    created: bool,
    detected_adapters: Vec<String>,
    generated_tasks: Vec<String>,
    error: String,
) -> RepositoryEntry {
    RepositoryEntry {
        path,
        configured: true,
        created,
        status: "invalid",
        detected_adapters,
        generated_tasks,
        doctor_errors: 0,
        missing_toolchains: Vec::new(),
        error: Some(error),
    }
}

fn repository_roots(parent: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = fs::read_dir(parent)
        .with_context(|| format!("could not read {}", parent.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(".git").exists())
        .collect::<Vec<_>>();
    roots.sort();
    if roots.is_empty() && parent.join(".git").exists() {
        roots.push(parent.to_path_buf());
    }
    Ok(roots)
}

fn print_report(report: &AdoptionReport, format: AdoptionFormat) -> Result<()> {
    match format {
        AdoptionFormat::Json => println!("{}", serde_json::to_string_pretty(report)?),
        AdoptionFormat::Pretty => {
            println!("Repositories: {}", report.parent);
            println!();
            for entry in &report.repositories {
                let marker = match entry.status {
                    "ready" => "✓",
                    "needs_configuration" => "–",
                    _ => "!",
                };
                let created = if entry.created {
                    " (created quality.yml)"
                } else {
                    ""
                };
                println!("  {marker} {:<24} {}{created}", entry.path, entry.status);
                if let Some(error) = &entry.error {
                    println!("    {error}");
                }
                if !entry.missing_toolchains.is_empty() {
                    println!("    missing: {}", entry.missing_toolchains.join(", "));
                }
            }
            println!();
            println!(
                "{} repositories: {} configured, {} need configuration, {} missing toolchains, {} invalid, {} created",
                report.summary.total,
                report.summary.configured,
                report.summary.needs_configuration,
                report.summary.missing_toolchains,
                report.summary.invalid,
                report.summary.created,
            );
        }
    }
    Ok(())
}
