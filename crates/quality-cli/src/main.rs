mod atomic;
mod baseline;
mod changes;
mod cli;
mod config;
mod hooks;
mod local_ci;
mod output;
mod presets;
mod project;
mod repositories;
mod runner;
mod tools;

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};

use crate::cli::{
    AdapterSelection, BaselineCommand, CiCommand, Cli, Command, HooksCommand, InstructionsFormat,
    PresetCommand,
};
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
    if let Command::Repositories { command } = cli.command {
        return repositories::run(&root, command);
    }
    let project = Project::discover(&root)?;

    match cli.command {
        Command::Init {
            force,
            dry_run,
            gate,
        } => {
            let path = root.join("quality.yml");
            if dry_run {
                print!("{}", config::initial_text_with_gate(&project, gate)?);
            } else {
                config::write_initial_with_gate(&path, &project, force, gate)?;
                println!("Created {}", display_path(&path));
                println!("Next: quality doctor && quality check");
            }
        }
        Command::Preset { command } => match command {
            PresetCommand::List => presets::print_list(),
            PresetCommand::Show { profile } => presets::print_profile(profile),
            PresetCommand::Apply {
                profile,
                dry_run,
                force,
                install,
                only,
                gate,
            } => presets::apply(&project, profile, &only, gate, dry_run, force, install)?,
            PresetCommand::Diff => {
                if presets::diff(&project)? {
                    std::process::exit(1);
                }
            }
            PresetCommand::Update {
                dry_run,
                force,
                install,
            } => presets::update(&project, dry_run, force, install)?,
            PresetCommand::Setup { install } => presets::setup(&project, install)?,
        },
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
            execution,
            format,
            report: report_path,
            fail_fast,
            changed,
            report_level,
            fail_level,
            require_checks,
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
                execution_settings(execution, require_checks),
            )?;
            baseline::apply(&mut report, &config.baseline_path(&root))?;
            present_run(
                &root,
                &report,
                runner::Operation::Check,
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
            execution,
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
                execution_settings(execution, false),
            )?;
            present_run(
                &root,
                &report,
                operation,
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
            execution,
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
                execution_settings(execution, false),
            )?;
            present_run(
                &root,
                &report,
                runner::Operation::Fix,
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
            command,
            force: legacy_force,
            install: legacy_install,
            shared_ref: legacy_shared_ref,
            shared_command: legacy_shared_command,
        } => match command {
            Some(CiCommand::Github {
                force,
                install,
                shared_ref,
                command,
            }) => {
                let path = generate_github_workflow(
                    &root,
                    &project,
                    force,
                    install.as_deref(),
                    shared_ref.as_deref(),
                    command.as_deref(),
                )?;
                println!("Created {}", display_path(&path));
            }
            Some(CiCommand::Plan {
                hook,
                format,
                strict,
            }) => {
                let config = Config::load_or_default(&root)?;
                let plan = local_ci::plan(&root, &config, &hook)?;
                local_ci::print_plan(&plan, format)?;
                if strict && plan.summary.uncovered > 0 {
                    std::process::exit(1);
                }
            }
            Some(CiCommand::Local {
                hook,
                step,
                format,
                report,
                no_history,
                max_output_bytes,
                args,
            }) => {
                let config = Config::load_or_default(&root)?;
                let report_data = local_ci::execute(
                    &root,
                    &config,
                    &hook,
                    step.map(std::num::NonZeroUsize::get),
                    &args,
                    max_output_bytes.get(),
                    matches!(format, cli::CiOutputFormat::Pretty),
                )?;
                if let Some(path) = report {
                    let path = if path.is_absolute() {
                        path
                    } else {
                        root.join(path)
                    };
                    local_ci::write_report(&report_data, &path)?;
                    eprintln!("Wrote local CI report to {}", display_path(&path));
                }
                if !no_history {
                    retain_local_ci_history(&root, &config, &report_data);
                }
                local_ci::print_report(&report_data, format)?;
                if !report_data.passed() {
                    std::process::exit(1);
                }
            }
            None => {
                if legacy_install.is_none() && legacy_shared_ref.is_none() {
                    anyhow::bail!(
                        "missing CI command; use `quality ci github --install COMMAND` or `quality ci plan`"
                    );
                }
                let path = generate_github_workflow(
                    &root,
                    &project,
                    legacy_force,
                    legacy_install.as_deref(),
                    legacy_shared_ref.as_deref(),
                    legacy_shared_command.as_deref(),
                )?;
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
                    runner::ExecutionSettings::default(),
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
        Command::Repositories { .. } => {
            unreachable!("repositories return before project discovery")
        }
        Command::Hooks { command } => {
            let config = Config::load_or_default(&root)?;
            match command {
                HooksCommand::Install => hooks::install(&root, &config)?,
                HooksCommand::Status => hooks::status(&root, &config)?,
                HooksCommand::Uninstall => hooks::uninstall(&root, &config)?,
                HooksCommand::Run { event, args } => {
                    let report =
                        local_ci::execute(&root, &config, &event, None, &args, 1024 * 1024, true)?;
                    retain_local_ci_history(&root, &config, &report);
                    local_ci::print_report(&report, cli::CiOutputFormat::Pretty)?;
                    if !report.passed() {
                        std::process::exit(1);
                    }
                }
            }
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

fn execution_settings(
    options: cli::ExecutionOptions,
    require_checks: bool,
) -> runner::ExecutionSettings {
    let defaults = runner::ExecutionSettings::default();
    runner::ExecutionSettings {
        jobs: options
            .jobs
            .map_or(defaults.jobs, std::num::NonZeroUsize::get),
        timeout_seconds: options.timeout_seconds.map(std::num::NonZeroU64::get),
        max_output_bytes: options.max_output_bytes.get(),
        require_checks,
    }
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
    operation: runner::Operation,
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
    output::print_run(run_report, operation, format, report_level, fail_level)
}

fn retain_local_ci_history(
    root: &std::path::Path,
    config: &Config,
    report: &local_ci::LocalCiReport,
) {
    if let Err(error) = local_ci::retain_report(root, config, report) {
        eprintln!("Warning: could not retain local CI history: {error:#}");
    }
}

fn generate_github_workflow(
    root: &std::path::Path,
    project: &Project,
    force: bool,
    install_command: Option<&str>,
    shared_ref: Option<&str>,
    shared_command: Option<&str>,
) -> Result<PathBuf> {
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
    let workflow = if let Some(reference) = shared_ref {
        if install_command.is_some() {
            anyhow::bail!("--install and --shared-ref cannot be used together");
        }
        let command = shared_command
            .ok_or_else(|| anyhow::anyhow!("--command is required with --shared-ref"))?;
        render_shared_github_workflow(root, reference, command)?
    } else {
        let install_command = install_command.ok_or_else(|| {
            anyhow::anyhow!("--install is required unless --shared-ref is provided")
        })?;
        validate_single_line("--install", install_command)?;
        let runner = if project.has_file("Package.swift") || project.path_contains(".xcodeproj/") {
            "macos-latest"
        } else {
            "ubuntu-latest"
        };
        let setup = github_project_setup(project, install_command);
        include_str!("../../../templates/github-actions.yml")
            .replace("__QUALITY_RUNNER__", runner)
            .replace("__QUALITY_PROJECT_SETUP__", &setup)
            .replace("__QUALITY_INSTALL_COMMAND__", install_command)
    };
    atomic::write(&path, workflow.as_bytes())
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(path)
}

fn render_shared_github_workflow(
    root: &std::path::Path,
    reference: &str,
    command: &str,
) -> Result<String> {
    validate_single_line("--shared-ref", reference)?;
    validate_single_line("--command", command)?;
    if reference.contains('@') || reference.chars().any(char::is_whitespace) {
        anyhow::bail!("--shared-ref must be a tag or commit without whitespace or @");
    }
    if !root.join("pnpm-lock.yaml").is_file() {
        anyhow::bail!("--shared-ref requires a pnpm-lock.yaml repository");
    }
    let node_input = if root.join(".node-version").is_file() {
        "      node-version-file: .node-version"
    } else {
        "      node-version: \"24\""
    };
    let pnpm_input = if root_package_manager_declares_pnpm(root)? {
        ""
    } else {
        "      pnpm-version: \"11.22.0\""
    };
    let command = serde_json::to_string(command)?;

    Ok(include_str!("../../../templates/github-actions-shared.yml")
        .replace("__QUALITY_SHARED_REF__", reference)
        .replace("__QUALITY_NODE_INPUT__", node_input)
        .replace("__QUALITY_PNPM_INPUT__", pnpm_input)
        .replace("__QUALITY_SHARED_COMMAND__", &command))
}

fn root_package_manager_declares_pnpm(root: &std::path::Path) -> Result<bool> {
    let manifest_path = root.join("package.json");
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("could not read {}", manifest_path.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .with_context(|| format!("could not parse {}", manifest_path.display()))?;
    Ok(manifest
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.starts_with("pnpm@")))
}

fn validate_single_line(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\n') || value.contains('\r') {
        anyhow::bail!("{name} must be one non-empty command line");
    }
    Ok(())
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

#[cfg(test)]
mod shared_workflow_tests {
    use super::validate_single_line;

    #[test]
    fn rejects_empty_or_multiline_shared_values() {
        assert!(validate_single_line("--command", "").is_err());
        assert!(validate_single_line("--command", "pnpm run check\npnpm test").is_err());
        assert!(validate_single_line("--command", "pnpm run verify").is_ok());
    }
}
