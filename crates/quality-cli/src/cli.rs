use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
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
    },
    /// Check installed tools and project configuration.
    Doctor {
        #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
        format: OutputFormat,
    },
    /// Run every applicable analyzer.
    Check {
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
    },
    /// Format the project or verify formatting without changing files.
    Format {
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

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CiProvider {
    Github,
}
