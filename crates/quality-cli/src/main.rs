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

use crate::cli::{AdapterSelection, BaselineCommand, CiProvider, Cli, Command, InstructionsFormat};
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
    if let Command::Instructions { format } = &cli.command {
        match format {
            InstructionsFormat::Agents => {
                print!(
                    "{}",
                    include_str!("../../../templates/agent-instructions.md")
                );
            }
        }
        return Ok(());
    }
    let root = cli
        .root
        .unwrap_or(env::current_dir().context("could not determine the current directory")?)
        .canonicalize()
        .context("project root does not exist")?;
    let project = Project::discover(&root)?;

    match cli.command {
        Command::Init { force, dry_run } => {
            let path = root.join("quality.yml");
            if dry_run {
                print!("{}", config::initial_text(&project)?);
            } else {
                config::write_initial(&path, &project, force)?;
                println!("Created {}", display_path(&path));
                println!("Next: quality doctor && quality check");
            }
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
            adapters,
            format,
            report: report_path,
            fail_fast,
            changed,
            report_level,
            fail_level,
        } => {
            let config = Config::load_or_default(&root)?;
            let adapters = prepare_selection(&config, adapters)?;
            let changes = discover_changes(&root, changed.as_deref())?;
            let mut report = runner::execute(
                &project,
                &config,
                runner::Operation::Check,
                fail_fast,
                changes.as_ref(),
                &adapters,
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
            adapters,
            check,
            format,
            report: report_path,
            changed,
        } => {
            let config = Config::load_or_default(&root)?;
            let adapters = prepare_selection(&config, adapters)?;
            let operation = if check {
                runner::Operation::CheckFormat
            } else {
                runner::Operation::Format
            };
            let changes = discover_changes(&root, changed.as_deref())?;
            let report = runner::execute(
                &project,
                &config,
                operation,
                false,
                changes.as_ref(),
                &adapters,
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
        Command::Fix {
            adapters,
            format,
            report: report_path,
            changed,
        } => {
            let config = Config::load_or_default(&root)?;
            let adapters = prepare_selection(&config, adapters)?;
            let changes = discover_changes(&root, changed.as_deref())?;
            let report = runner::execute(
                &project,
                &config,
                runner::Operation::Fix,
                false,
                changes.as_ref(),
                &adapters,
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
                let path = generate_github_workflow(&root, &project, force, &install)?;
                println!("Created {}", display_path(&path));
            }
        },
        Command::Baseline { command } => match command {
            BaselineCommand::Create { output, force } => {
                let config = Config::load_or_default(&root)?;
                let report = runner::execute(
                    &project,
                    &config,
                    runner::Operation::Check,
                    false,
                    None,
                    &AdapterSelection::default(),
                )?;
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
        Command::Instructions { .. } => {
            unreachable!("instructions return before project discovery")
        }
    }

    Ok(())
}

fn prepare_selection(config: &Config, mut selection: AdapterSelection) -> Result<AdapterSelection> {
    selection.normalize();
    let ids: Vec<_> = selection
        .only
        .iter()
        .chain(&selection.exclude)
        .cloned()
        .collect();
    config.validate_adapter_selection(&ids)?;
    Ok(selection)
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
    project: &Project,
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
    let runner = if project.has_file("Package.swift") || project.path_contains(".xcodeproj/") {
        "macos-latest"
    } else {
        "ubuntu-latest"
    };
    let setup = github_project_setup(project, install_command);
    let workflow = include_str!("../../../templates/github-actions.yml")
        .replace("__QUALITY_RUNNER__", runner)
        .replace("__QUALITY_PROJECT_SETUP__", &setup)
        .replace("__QUALITY_INSTALL_COMMAND__", install_command);
    std::fs::write(&path, workflow)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

fn github_project_setup(project: &Project, install_command: &str) -> String {
    let mut steps = Vec::new();
    if install_command
        .split_ascii_whitespace()
        .next()
        .is_some_and(|command| command == "cargo")
        || project.has_file("Cargo.toml")
    {
        steps.push(
            "      - name: Install Rust\n        uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c # stable",
        );
    }

    if project.has_file("pnpm-lock.yaml") {
        steps.push(
            "      - name: Install pnpm\n        uses: pnpm/action-setup@0977fd99725f1db4007ccb2928dbb4e90d06cc86 # v6\n        with:\n          run_install: false",
        );
        steps.push(
            "      - name: Set up Node.js\n        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7\n        with:\n          node-version: lts/*\n          cache: pnpm",
        );
        steps.push(
            "      - name: Install dependencies\n        run: pnpm install --frozen-lockfile",
        );
    } else if project.has_file("yarn.lock") {
        steps.push(
            "      - name: Set up Node.js\n        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7\n        with:\n          node-version: lts/*\n          cache: yarn",
        );
        steps.push(
            "      - name: Install dependencies\n        run: |\n          corepack enable\n          yarn install --immutable",
        );
    } else if project.has_file("package-lock.json") {
        steps.push(
            "      - name: Set up Node.js\n        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7\n        with:\n          node-version: lts/*\n          cache: npm",
        );
        steps.push("      - name: Install dependencies\n        run: npm ci");
    } else if project.has_file("bun.lock") || project.has_file("bun.lockb") {
        steps.push("      - name: Set up Bun\n        uses: oven-sh/setup-bun@v2");
        steps
            .push("      - name: Install dependencies\n        run: bun install --frozen-lockfile");
    }

    if project.has_file("AndroidManifest.xml") {
        steps.push(
            "      - name: Set up Java\n        uses: actions/setup-java@v5\n        with:\n          distribution: temurin\n          java-version: '17'\n          cache: gradle",
        );
    }
    if detected_tool(project, "actionlint") {
        steps.push(
            "      - name: Install actionlint\n        run: go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12",
        );
    }
    if detected_tool(project, "swiftlint") {
        steps.push("      - name: Install SwiftLint\n        run: brew install swiftlint");
    }
    if detected_tool(project, "swiftformat") {
        steps.push("      - name: Install SwiftFormat\n        run: brew install swiftformat");
    }

    if steps.is_empty() {
        "      # No project toolchain setup was detected.".to_owned()
    } else {
        steps.join("\n\n")
    }
}

fn detected_tool(project: &Project, id: &str) -> bool {
    tools::catalog()
        .into_iter()
        .find(|tool| tool.id == id)
        .is_some_and(|tool| tool.detect(project))
}

fn display_path(path: &std::path::Path) -> String {
    path.strip_prefix(env::current_dir().unwrap_or_default())
        .unwrap_or(path)
        .display()
        .to_string()
}
