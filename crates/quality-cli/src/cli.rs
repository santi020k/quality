use std::ffi::OsString;
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(
    name = "quality",
    version,
    about = "One code-quality workflow for every project"
)]
pub struct Cli {
    /// Project directory. Defaults to the current directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a friendly starter configuration.
    Init {
        /// Replace an existing quality.yml.
        #[arg(long)]
        force: bool,
        /// Print the detected configuration without writing a file.
        #[arg(long)]
        dry_run: bool,
        /// Choose which repository script becomes the generated canonical gate.
        #[arg(long, value_enum, default_value_t = GateProfile::Auto)]
        gate: GateProfile,
    },
    /// Inspect or apply language-aware quality presets.
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
    /// Check installed tools and project configuration.
    Doctor {
        #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
        format: OutputFormat,
    },
    /// Run every applicable analyzer.
    Check {
        #[command(flatten)]
        adapters: AdapterSelection,
        #[command(flatten)]
        execution: ExecutionOptions,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Also write a SARIF report without changing console output.
        #[arg(long, value_name = "PATH")]
        report: Option<PathBuf>,
        /// Stop after the first failure instead of running tools concurrently.
        #[arg(long)]
        fail_fast: bool,
        /// Check files changed since BASE. With no BASE, include local changes since HEAD.
        #[arg(long, num_args = 0..=1, default_missing_value = "HEAD", value_name = "BASE")]
        changed: Option<String>,
        /// Lowest severity shown in human, GitHub, and SARIF reports.
        #[arg(long, value_enum, default_value_t = Severity::Info)]
        report_level: Severity,
        /// Lowest diagnostic severity that makes the command fail.
        #[arg(long, value_enum, default_value_t = Severity::Info)]
        fail_level: Severity,
        /// Fail when no check adapters are configured, while allowing changed scopes to skip work.
        #[arg(long)]
        require_checks: bool,
    },
    /// Format the project or verify formatting without changing files.
    Format {
        #[command(flatten)]
        adapters: AdapterSelection,
        #[command(flatten)]
        execution: ExecutionOptions,
        /// Only check formatting; do not change files.
        #[arg(long)]
        check: bool,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Also write a SARIF report without changing console output.
        #[arg(long, value_name = "PATH")]
        report: Option<PathBuf>,
        /// Format files changed since BASE. With no BASE, include local changes since HEAD.
        #[arg(long, num_args = 0..=1, default_missing_value = "HEAD", value_name = "BASE")]
        changed: Option<String>,
    },
    /// Apply safe fixes exposed by configured tools.
    Fix {
        #[command(flatten)]
        adapters: AdapterSelection,
        #[command(flatten)]
        execution: ExecutionOptions,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Also write a SARIF report without changing console output.
        #[arg(long, value_name = "PATH")]
        report: Option<PathBuf>,
        /// Fix files changed since BASE. With no BASE, include local changes since HEAD.
        #[arg(long, num_args = 0..=1, default_missing_value = "HEAD", value_name = "BASE")]
        changed: Option<String>,
    },
    /// Record existing findings so adoption can focus on new regressions.
    Baseline {
        #[command(subcommand)]
        command: BaselineCommand,
    },
    /// Generate tab-completion scripts for your shell.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print instructions for AI coding agents.
    Instructions {
        #[arg(long, value_enum, default_value_t = InstructionsFormat::Agents)]
        format: InstructionsFormat,
    },
    /// Generate continuous-integration configuration.
    Ci {
        #[arg(value_enum, default_value_t = CiProvider::Github)]
        provider: CiProvider,
        /// Replace an existing workflow.
        #[arg(long)]
        force: bool,
        /// Command CI should run to install quality (for example, a pinned Cargo --git command).
        #[arg(long, value_name = "COMMAND")]
        install: String,
    },
    /// Audit or configure a folder containing multiple Git repositories.
    Repositories {
        #[command(subcommand)]
        command: RepositoriesCommand,
    },
    /// Install and run package-manager-independent Git hooks.
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List the built-in preset profiles.
    List,
    /// Explain a preset and preview its language policy.
    Show {
        #[arg(value_enum)]
        profile: PresetProfile,
    },
    /// Generate configs for the languages detected in this repository.
    Apply {
        #[arg(value_enum)]
        profile: PresetProfile,
        /// Preview every file and dependency command without changing the repository.
        #[arg(long)]
        dry_run: bool,
        /// Replace preset-owned files that already contain different content.
        #[arg(long)]
        force: bool,
        /// Install pinned JavaScript development dependencies with the detected package manager.
        #[arg(long)]
        install: bool,
        /// Limit generation to ecosystems. Repeat or comma-separate values.
        #[arg(long, value_enum, value_name = "ECOSYSTEM", value_delimiter = ',')]
        only: Vec<PresetEcosystem>,
        /// Choose which repository script becomes the generated canonical gate.
        #[arg(long, value_enum, default_value_t = GateProfile::Auto)]
        gate: GateProfile,
    },
    /// Compare the applied preset with the current built-in catalog.
    Diff,
    /// Refresh an applied preset while preserving user-owned changes.
    Update {
        /// Preview changes without writing files or installing dependencies.
        #[arg(long)]
        dry_run: bool,
        /// Replace generated files that were edited after the preset was applied.
        #[arg(long)]
        force: bool,
        /// Install missing or outdated JavaScript development dependencies.
        #[arg(long)]
        install: bool,
    },
    /// Explain or install native tools required by the applied preset.
    Setup {
        /// Run supported setup commands instead of only printing them.
        #[arg(long)]
        install: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum PresetProfile {
    Minimal,
    Recommended,
    Strict,
}

impl std::fmt::Display for PresetProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Minimal => "minimal",
            Self::Recommended => "recommended",
            Self::Strict => "strict",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum PresetEcosystem {
    #[value(name = "javascript")]
    JavaScript,
    Python,
    Rust,
    Swift,
    Kotlin,
    GithubActions,
}

impl std::fmt::Display for PresetEcosystem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::GithubActions => "github-actions",
        })
    }
}

#[derive(Clone, Copy, Debug, Args)]
pub struct ExecutionOptions {
    /// Maximum analyzer processes to run concurrently.
    #[arg(long, value_name = "COUNT")]
    pub jobs: Option<NonZeroUsize>,
    /// Override every configured adapter timeout.
    #[arg(long, value_name = "SECONDS")]
    pub timeout_seconds: Option<NonZeroU64>,
    /// Maximum bytes retained from each analyzer's combined output.
    #[arg(long, value_name = "BYTES", default_value = "1048576")]
    pub max_output_bytes: NonZeroUsize,
}

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Install small managed hook launchers for the events in quality.yml.
    Install,
    /// Report whether every configured hook is installed.
    Status,
    /// Remove only hook launchers managed by quality.
    Uninstall,
    /// Run the configured steps for one Git hook event.
    Run {
        /// Git hook name, such as pre-commit, commit-msg, or pre-push.
        event: String,
        /// Arguments supplied by Git to the hook.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum GateProfile {
    #[default]
    Auto,
    Fast,
    Full,
}

#[derive(Debug, Subcommand)]
pub enum RepositoriesCommand {
    /// Report adoption readiness without changing repositories.
    Audit {
        #[arg(long, value_enum, default_value_t = AdoptionFormat::Pretty)]
        format: AdoptionFormat,
        /// Exit unsuccessfully when a selected condition is found. Repeat or comma-separate values.
        #[arg(long, value_enum, value_name = "CONDITION", value_delimiter = ',')]
        fail_on: Vec<AuditFailureCondition>,
    },
    /// Create quality.yml in repositories that do not have one.
    Apply {
        /// Preview which configurations would be created.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = AdoptionFormat::Pretty)]
        format: AdoptionFormat,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AuditFailureCondition {
    Invalid,
    MissingConfiguration,
    MissingToolchain,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum AdoptionFormat {
    #[default]
    Pretty,
    Json,
}

#[derive(Clone, Debug, Default, Args)]
pub struct AdapterSelection {
    /// Run only the named adapter. Repeat the flag or separate IDs with commas.
    #[arg(long, value_name = "ID", value_delimiter = ',')]
    pub only: Vec<String>,
    /// Skip the named adapter. Repeat the flag or separate IDs with commas.
    #[arg(long, value_name = "ID", value_delimiter = ',')]
    pub exclude: Vec<String>,
}

impl AdapterSelection {
    pub fn is_empty(&self) -> bool {
        self.only.is_empty() && self.exclude.is_empty()
    }

    pub fn includes(&self, id: &str) -> bool {
        (self.only.is_empty() || self.only.iter().any(|selected| selected == id))
            && !self.exclude.iter().any(|excluded| excluded == id)
    }

    pub fn normalize(&mut self) {
        self.only.sort();
        self.only.dedup();
        self.exclude.sort();
        self.exclude.dedup();
    }
}

#[derive(Debug, Subcommand)]
pub enum BaselineCommand {
    /// Run all checks and record their current file findings.
    Create {
        /// Write to a path other than the configured baseline.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Replace an existing baseline.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Pretty,
    Json,
    Sarif,
    /// Emit GitHub Actions workflow commands for inline annotations.
    Github,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn includes(self, severity: &str) -> bool {
        severity_rank(severity) >= self as u8
    }
}

fn severity_rank(severity: &str) -> u8 {
    match severity.to_ascii_lowercase().as_str() {
        "error" => Severity::Error as u8,
        "warning" => Severity::Warning as u8,
        _ => Severity::Info as u8,
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CiProvider {
    Github,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum InstructionsFormat {
    #[default]
    Agents,
}
