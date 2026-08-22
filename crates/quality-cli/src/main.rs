mod baseline;
mod changes;
mod cli;
mod config;
mod output;
mod project;
mod runner;
mod tools;

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};

use crate::cli::{BaselineCommand, CiProvider, Cli, Command};
use crate::config::Config;
use crate::project::Project;

fn main() {
    if let Err(error) = run() {
        eprintln!("quality: {error:#}");
        std::process::exit(2);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Command::Completions { shell } = &cli.command {
        clap_complete::generate(
            *shell,
            &mut Cli::command(),
            "quality",
            &mut std::io::stdout(),
        );
        return Ok(());
    }
    let root = cli
        .root
        .unwrap_or(env::current_dir().context("could not determine the current directory")?)
        .canonicalize()
        .context("project root does not exist")?;
    let project = Project::discover(&root)?;

    match cli.command {
        Command::Init { force } => {
            let path = root.join("quality.yml");
            config::write_initial(&path, &project, force)?;
            println!("Created {}", display_path(&path));
            println!("Next: quality doctor && quality check");
        }
        Command::Doctor { format } => {
            let config = Config::load_or_default(&root)?;
            let report = runner::doctor(&project, &config);
            output::print_doctor(&report, format)?;
            if report.has_errors() {
                std::process::exit(1);
            }
        }
        Command::Check {
            format,
            report: report_path,
            fail_fast,
            changed,
            report_level,
            fail_level,
        } => {
            let config = Config::load_or_default(&root)?;
            let changes = discover_changes(&root, changed.as_deref())?;
            let mut report = runner::execute(
                &project,
                &config,
                runner::Operation::Check,
                fail_fast,
                changes.as_ref(),
            )?;
            baseline::apply(&mut report, &config.baseline_path(&root))?;
            present_run(
                &root,
                &report,
                format.unwrap_or(config.output_format()),
                report_path,
                report_level,
                fail_level,
            )?;
            if report.failed_at(fail_level) {
                std::process::exit(1);
            }
        }
        Command::Format {
            check,
            format,
            report: report_path,
            changed,
        } => {
            let config = Config::load_or_default(&root)?;
            let operation = if check {
                runner::Operation::CheckFormat
            } else {
                runner::Operation::Format
            };
            let changes = discover_changes(&root, changed.as_deref())?;
            let report = runner::execute(&project, &config, operation, false, changes.as_ref())?;
            present_run(
                &root,
                &report,
                format.unwrap_or(config.output_format()),
                report_path,
                cli::Severity::Info,
                cli::Severity::Info,
            )?;
            if report.failed() {
                std::process::exit(1);
            }
        }
        Command::Fix {
            format,
            report: report_path,
            changed,
        } => {
            let config = Config::load_or_default(&root)?;
            let changes = discover_changes(&root, changed.as_deref())?;
            let report = runner::execute(
                &project,
                &config,
                runner::Operation::Fix,
                false,
                changes.as_ref(),
            )?;
            present_run(
                &root,
                &report,
                format.unwrap_or(config.output_format()),
                report_path,
                cli::Severity::Info,
                cli::Severity::Info,
            )?;
            if report.failed() {
                std::process::exit(1);
            }
        }
        Command::Ci {
            provider,
            force,
            install,
        } => match provider {
            CiProvider::Github => {
                let path = generate_github_workflow(&root, force, &install)?;
                println!("Created {}", display_path(&path));
            }
        },
        Command::Baseline { command } => match command {
            BaselineCommand::Create { output, force } => {
                let config = Config::load_or_default(&root)?;
                let report =
                    runner::execute(&project, &config, runner::Operation::Check, false, None)?;
                let path = output
                    .map(|path| {
                        if path.is_absolute() {
                            path
                        } else {
                            root.join(path)
                        }
                    })
                    .unwrap_or_else(|| config.baseline_path(&root));
                let summary = baseline::create(&report, &path, force)?;
                println!(
                    "Created {} with {} findings ({} occurrences)",
                    display_path(&path),
                    summary.findings,
                    summary.occurrences
                );
                println!("Commit this file; future checks will report only new findings.");
            }
        },
        Command::Completions { .. } => unreachable!("completions return before project discovery"),
    }

    Ok(())
}

fn discover_changes(
    root: &std::path::Path,
    base: Option<&str>,
) -> Result<Option<changes::ChangeSet>> {
    base.map(|base| changes::discover(root, base)).transpose()
}

fn present_run(
    root: &std::path::Path,
    run_report: &runner::RunReport,
    format: cli::OutputFormat,
    report_path: Option<PathBuf>,
    report_level: cli::Severity,
    fail_level: cli::Severity,
) -> Result<()> {
    if let Some(report_path) = report_path {
        let report_path = if report_path.is_absolute() {
            report_path
        } else {
            root.join(report_path)
        };
        output::write_sarif(run_report, &report_path, report_level)?;
        eprintln!("Wrote SARIF report to {}", display_path(&report_path));
    }
    output::print_run(run_report, format, report_level, fail_level)
}

fn generate_github_workflow(
    root: &std::path::Path,
    force: bool,
    install_command: &str,
) -> Result<PathBuf> {
    if install_command.trim().is_empty()
        || install_command.contains('\n')
        || install_command.contains('\r')
    {
        anyhow::bail!("--install must be one non-empty command line");
    }
    let path = root.join(".github/workflows/quality.yml");
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to replace it",
            display_path(&path)
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let workflow = include_str!("../../../templates/github-actions.yml")
        .replace("__QUALITY_INSTALL_COMMAND__", install_command);
    std::fs::write(&path, workflow)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

fn display_path(path: &std::path::Path) -> String {
    path.strip_prefix(env::current_dir().unwrap_or_default())
        .unwrap_or(path)
        .display()
        .to_string()
}
