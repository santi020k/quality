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
    },
    /// Format the project or verify formatting without changing files.
    Format {
        #[command(flatten)]
        adapters: AdapterSelection,
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
