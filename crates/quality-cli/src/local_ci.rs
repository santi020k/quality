use std::collections::{BTreeMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::CiOutputFormat;
use crate::config::{Config, HookStepConfig};
use crate::runner::{self, Diagnostic};

const REPORT_SCHEMA_VERSION: u8 = 1;
const PLAN_SCHEMA_VERSION: u8 = 1;
const RETAINED_RUNS: usize = 20;

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiPlan {
    pub schema_version: u8,
    pub hook: String,
    pub local_steps: Vec<LocalStepPlan>,
    pub workflow_steps: Vec<WorkflowStepPlan>,
    pub summary: PlanSummary,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalStepPlan {
    pub number: usize,
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub covers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkflowStepPlan {
    pub workflow: String,
    pub job: String,
    pub name: String,
    pub status: WorkflowStepStatus,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    Covered,
    GithubOnly,
    Uncovered,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PlanSummary {
    pub local_steps: usize,
    pub covered: usize,
    pub github_only: usize,
    pub uncovered: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalCiReport {
    pub schema_version: u8,
    pub hook: String,
    pub started_at_unix_ms: u128,
    pub duration_ms: u128,
    pub status: LocalCiStatus,
    pub steps: Vec<LocalCiStepResult>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCiStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalCiStepResult {
    pub number: usize,
    pub name: String,
    pub status: LocalCiStatus,
    pub duration_ms: u128,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub output_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiHistory {
    pub schema_version: u8,
    pub runs: Vec<LocalCiHistoryEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiHistoryEntry {
    pub id: String,
    pub hook: String,
    pub started_at_unix_ms: u128,
    pub duration_ms: u128,
    pub status: LocalCiStatus,
    pub steps: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiComparison {
    pub schema_version: u8,
    pub base: LocalCiHistoryEntry,
    pub head: LocalCiHistoryEntry,
    pub duration_delta_ms: i128,
    pub duration_delta_percent: Option<f64>,
    pub steps: Vec<LocalCiStepComparison>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiStepComparison {
    pub name: String,
    pub command: String,
    pub base_duration_ms: Option<u128>,
    pub head_duration_ms: Option<u128>,
    pub duration_delta_ms: Option<i128>,
    pub base_status: Option<LocalCiStatus>,
    pub head_status: Option<LocalCiStatus>,
}

impl LocalCiReport {
    pub fn passed(&self) -> bool {
        self.status == LocalCiStatus::Passed
    }
}

pub fn plan(root: &Path, config: &Config, hook_name: &str) -> Result<LocalCiPlan> {
    let hook = config
        .hooks
        .get(hook_name)
        .with_context(|| format!("hook `{hook_name}` is not configured in quality.yml"))?;
    let local_steps = hook
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| LocalStepPlan {
            number: index + 1,
            name: step_name(step),
            command: command_display(step, &[]),
            working_directory: step
                .working_directory
                .as_ref()
                .map(|path| path.display().to_string()),
            covers: step.covers.clone(),
        })
        .collect::<Vec<_>>();
    let workflow_steps = inspect_pull_request_workflows(root, &hook.steps)?;
    let summary = PlanSummary {
        local_steps: local_steps.len(),
        covered: workflow_steps
            .iter()
            .filter(|step| matches!(step.status, WorkflowStepStatus::Covered))
            .count(),
        github_only: workflow_steps
            .iter()
            .filter(|step| matches!(step.status, WorkflowStepStatus::GithubOnly))
            .count(),
        uncovered: workflow_steps
            .iter()
            .filter(|step| matches!(step.status, WorkflowStepStatus::Uncovered))
            .count(),
    };
    Ok(LocalCiPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        hook: hook_name.to_owned(),
        local_steps,
        workflow_steps,
        summary,
    })
}

pub fn print_plan(plan: &LocalCiPlan, format: CiOutputFormat) -> Result<()> {
    match format {
        CiOutputFormat::Json => println!("{}", serde_json::to_string_pretty(plan)?),
        CiOutputFormat::Pretty => {
            println!("Local CI plan ({})", plan.hook);
            println!();
            println!("Local gate:");
            for step in &plan.local_steps {
                println!("  {}. {}", step.number, step.name);
                println!("     {}", step.command);
                for covered in &step.covers {
                    println!("     covers: {covered}");
                }
            }
            if plan.workflow_steps.is_empty() {
                println!();
                println!("No pull-request workflow steps were found.");
            } else {
                println!();
                println!("Pull-request workflow coverage:");
                for step in &plan.workflow_steps {
                    let marker = match step.status {
                        WorkflowStepStatus::Covered => "✓",
                        WorkflowStepStatus::GithubOnly => "–",
                        WorkflowStepStatus::Uncovered => "!",
                    };
                    println!(
                        "  {marker} {} / {} / {} — {}",
                        step.workflow, step.job, step.name, step.detail
                    );
                }
            }
            println!();
            println!(
                "{} covered · {} GitHub-only · {} uncovered · {} local steps",
                plan.summary.covered,
                plan.summary.github_only,
                plan.summary.uncovered,
                plan.summary.local_steps
            );
        }
    }
    Ok(())
}

pub fn execute(
    root: &Path,
    config: &Config,
    hook_name: &str,
    selected_step: Option<usize>,
    hook_args: &[OsString],
    max_output_bytes: usize,
    show_progress: bool,
) -> Result<LocalCiReport> {
    if std::env::var_os("QUALITY_LOCAL_CI").is_some() {
        anyhow::bail!(
            "recursive local CI invocation blocked; remove `quality ci local` or `quality hooks run` from the configured hook command"
        );
    }
    let hook = config
        .hooks
        .get(hook_name)
        .with_context(|| format!("hook `{hook_name}` is not configured in quality.yml"))?;
    if let Some(number) = selected_step {
        if number == 0 || number > hook.steps.len() {
            anyhow::bail!(
                "step {number} does not exist in hook `{hook_name}`; choose 1 through {}",
                hook.steps.len()
            );
        }
    }

    let started_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let started = Instant::now();
    let mut failed = false;
    let mut results = Vec::with_capacity(hook.steps.len());

    for (index, step) in hook.steps.iter().enumerate() {
        let number = index + 1;
        if selected_step.is_some_and(|selected| selected != number) || failed {
            results.push(skipped_step(
                number,
                step,
                hook_name,
                step.pass_hook_args && !hook_args.is_empty(),
            ));
            continue;
        }
        if show_progress {
            eprintln!(
                "Running step {number}/{}: {}",
                hook.steps.len(),
                step_name(step)
            );
        }
        let result = execute_step(root, hook_name, number, step, hook_args, max_output_bytes);
        failed = result.status == LocalCiStatus::Failed;
        results.push(result);
    }

    let status = if failed {
        LocalCiStatus::Failed
    } else if results
        .iter()
        .all(|result| result.status == LocalCiStatus::Skipped)
    {
        LocalCiStatus::Skipped
    } else {
        LocalCiStatus::Passed
    };
    Ok(LocalCiReport {
        schema_version: REPORT_SCHEMA_VERSION,
        hook: hook_name.to_owned(),
        started_at_unix_ms,
        duration_ms: started.elapsed().as_millis(),
        status,
        steps: results,
    })
}

pub fn print_report(report: &LocalCiReport, format: CiOutputFormat) -> Result<()> {
    match format {
        CiOutputFormat::Json => println!("{}", serde_json::to_string_pretty(report)?),
        CiOutputFormat::Pretty => {
            println!("Local CI ({})", report.hook);
            println!();
            for step in &report.steps {
                let seconds = step.duration_ms as f64 / 1000.0;
                match step.status {
                    LocalCiStatus::Passed => {
                        println!("  ✓ {:<36} {:>8.2}s", step.name, seconds)
                    }
                    LocalCiStatus::Skipped => println!("  – {:<36}  skipped", step.name),
                    LocalCiStatus::Failed => {
                        println!("  ✗ {:<36} {:>8.2}s", step.name, seconds);
                        if let Some(failure) = &step.failure {
                            println!("    {failure}");
                        }
                        for diagnostic in step.diagnostics.iter().take(20) {
                            let location =
                                match (&diagnostic.path, diagnostic.line, diagnostic.column) {
                                    (Some(path), Some(line), Some(column)) => {
                                        format!("{path}:{line}:{column}")
                                    }
                                    (Some(path), Some(line), None) => format!("{path}:{line}"),
                                    (Some(path), None, None) => path.clone(),
                                    _ => step.name.clone(),
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
                        if step.diagnostics.len() > 20 {
                            println!("    … and {} more diagnostics", step.diagnostics.len() - 20);
                        }
                        if !step.output.trim().is_empty() {
                            println!();
                            let lines = step.output.trim_end().lines().collect::<Vec<_>>();
                            let omitted = lines.len().saturating_sub(80);
                            if omitted > 0 {
                                println!("    … {omitted} earlier output lines omitted");
                            }
                            for line in lines.iter().skip(omitted) {
                                println!("    {line}");
                            }
                            if step.output_truncated {
                                println!("    … output truncated");
                            }
                        }
                        if let Some(rerun) = &step.rerun {
                            println!();
                            println!("    Rerun: {rerun}");
                        }
                    }
                }
            }
            println!();
            let seconds = report.duration_ms as f64 / 1000.0;
            match report.status {
                LocalCiStatus::Passed => println!("Local CI passed in {seconds:.2}s wall time."),
                LocalCiStatus::Failed => println!("Local CI failed in {seconds:.2}s wall time."),
                LocalCiStatus::Skipped => println!("Local CI skipped in {seconds:.2}s wall time."),
            }
        }
    }
    Ok(())
}

pub fn write_report(report: &LocalCiReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create report directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_vec_pretty(report)?;
    crate::atomic::write(path, &serialized)
        .with_context(|| format!("could not write local CI report to {}", path.display()))
}

pub fn retain_report(
    root: &Path,
    config: &Config,
    report: &LocalCiReport,
) -> Result<Option<PathBuf>> {
    let Some(directory) = history_directory(root)? else {
        return Ok(None);
    };
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let mut retained = report.clone();
    let configured_steps = config
        .hooks
        .get(&report.hook)
        .map(|hook| hook.steps.as_slice())
        .unwrap_or_default();
    for (index, step) in retained.steps.iter_mut().enumerate() {
        step.output.clear();
        step.output_truncated = false;
        step.diagnostics.clear();
        if let Some(configured) = configured_steps.get(index) {
            step.command = command_display(configured, &[]);
        }
    }
    let run_path = write_history_report(&directory, &retained)?;
    write_report(&retained, &directory.join("latest.json"))?;
    prune_history(&directory)?;
    Ok(Some(run_path))
}

pub fn print_history(
    root: &Path,
    hook: Option<&str>,
    limit: usize,
    format: CiOutputFormat,
) -> Result<()> {
    let runs = read_history(root, hook)?
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    let entries = runs
        .iter()
        .map(|run| history_entry(&run.id, &run.report))
        .collect::<Vec<_>>();
    match format {
        CiOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&LocalCiHistory {
                schema_version: REPORT_SCHEMA_VERSION,
                runs: entries,
            })?
        ),
        CiOutputFormat::Pretty => {
            if entries.is_empty() {
                println!("No retained local CI runs found.");
            } else {
                println!("Local CI history");
                println!();
                for entry in entries {
                    println!(
                        "  {:<22} {:<12} {:<8} {:>8.2}s  {} step(s)",
                        entry.id,
                        entry.hook,
                        status_label(entry.status),
                        entry.duration_ms as f64 / 1000.0,
                        entry.steps
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn print_history_run(
    root: &Path,
    hook: Option<&str>,
    selector: &str,
    format: CiOutputFormat,
) -> Result<()> {
    let runs = read_history(root, hook)?;
    let run = select_history_run(&runs, selector)?;
    if matches!(format, CiOutputFormat::Pretty) {
        println!("Retained run {}", run.id);
        println!();
    }
    print_report(&run.report, format)
}

pub fn print_comparison(
    root: &Path,
    hook: Option<&str>,
    base_selector: Option<&str>,
    head_selector: Option<&str>,
    format: CiOutputFormat,
) -> Result<()> {
    let runs = read_history(root, hook)?;
    let head = select_history_run(&runs, head_selector.unwrap_or("latest"))?;
    let base = if let Some(selector) = base_selector {
        select_history_run(&runs, selector)?
    } else {
        previous_history_run(&runs, head)?
    };
    if base.id == head.id {
        anyhow::bail!("comparison requires two different retained runs");
    }
    if base.report.hook != head.report.hook {
        anyhow::bail!(
            "cannot compare hook `{}` with hook `{}`; pass --hook or choose matching runs",
            base.report.hook,
            head.report.hook
        );
    }
    let comparison = compare_runs(base, head);
    match format {
        CiOutputFormat::Json => println!("{}", serde_json::to_string_pretty(&comparison)?),
        CiOutputFormat::Pretty => print_comparison_pretty(&comparison),
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct StoredRun {
    id: String,
    report: LocalCiReport,
}

fn history_directory(root: &Path) -> Result<Option<PathBuf>> {
    let mut command = Command::new("git");
    command
        .args(["rev-parse", "--git-path", "quality/local-ci"])
        .current_dir(root);
    // Hooks can export another repository's paths. Resolve history from the
    // requested root while preserving the user's intentional Git configuration.
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(variable);
    }
    let output = command
        .output()
        .context("could not inspect the Git directory for local CI history")?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if raw.is_empty() {
        return Ok(None);
    }
    let directory = PathBuf::from(raw);
    let directory = if directory.is_absolute() {
        directory
    } else {
        root.join(directory)
    };
    Ok(Some(directory))
}

fn read_history(root: &Path, hook: Option<&str>) -> Result<Vec<StoredRun>> {
    let Some(directory) = history_directory(root)? else {
        return Ok(Vec::new());
    };
    read_history_in(&directory, hook)
}

fn read_history_in(directory: &Path, hook: Option<&str>) -> Result<Vec<StoredRun>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut runs = Vec::new();
    for entry in fs::read_dir(directory)
        .with_context(|| format!("could not read {}", directory.display()))?
    {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if !name.starts_with("run-") || !name.ends_with(".json") {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", path.display()));
            }
        };
        let report: LocalCiReport = serde_json::from_str(&text)
            .with_context(|| format!("invalid local CI history in {}", path.display()))?;
        if report.schema_version != REPORT_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported local CI history schema {} in {}; expected {}",
                report.schema_version,
                path.display(),
                REPORT_SCHEMA_VERSION
            );
        }
        if hook.is_some_and(|selected| selected != report.hook) {
            continue;
        }
        runs.push(StoredRun {
            id: name.trim_end_matches(".json").to_owned(),
            report,
        });
    }
    runs.sort_by(|left, right| {
        right
            .report
            .started_at_unix_ms
            .cmp(&left.report.started_at_unix_ms)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(runs)
}

fn write_history_report(directory: &Path, report: &LocalCiReport) -> Result<PathBuf> {
    let serialized = serde_json::to_vec_pretty(report)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory).with_context(|| {
        format!(
            "could not create temporary history in {}",
            directory.display()
        )
    })?;
    temporary
        .write_all(&serialized)
        .context("could not write temporary local CI history")?;
    temporary
        .as_file()
        .sync_all()
        .context("could not sync temporary local CI history")?;

    let started_at_unix_ms = report.started_at_unix_ms;
    for suffix in 0..=9999_u16 {
        let name = if suffix == 0 {
            format!("run-{started_at_unix_ms}.json")
        } else {
            format!("run-{started_at_unix_ms}-{suffix:04}.json")
        };
        let candidate = directory.join(name);
        // Publish the complete report without replacing a run created by a
        // concurrent process between filename selection and persistence.
        match temporary.persist_noclobber(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => {
                return Err(error.error).with_context(|| {
                    format!("could not retain history in {}", candidate.display())
                });
            }
        }
    }
    anyhow::bail!("could not allocate a unique local CI history file")
}

fn select_history_run<'a>(runs: &'a [StoredRun], selector: &str) -> Result<&'a StoredRun> {
    match selector {
        "latest" => runs
            .first()
            .with_context(|| "no retained local CI runs found"),
        "previous" => runs
            .get(1)
            .with_context(|| "no previous retained local CI run found"),
        value => {
            let without_suffix = value.strip_suffix(".json").unwrap_or(value);
            let normalized = without_suffix
                .strip_prefix("run-")
                .unwrap_or(without_suffix);
            runs.iter()
                .find(|run| run.id == value || run.id == format!("run-{normalized}"))
                .with_context(|| format!("retained local CI run `{value}` was not found"))
        }
    }
}

/// The newest earlier run of the same hook that executed the same commands in
/// the same order. Fail-fast and focused runs can both skip steps; compare what
/// actually ran instead of trying to infer which selection produced the report.
fn previous_history_run<'a>(runs: &'a [StoredRun], head: &StoredRun) -> Result<&'a StoredRun> {
    let selection = executed_steps(&head.report);
    runs.iter()
        .skip_while(|run| run.id != head.id)
        .skip(1)
        .find(|run| {
            run.report.hook == head.report.hook && executed_steps(&run.report) == selection
        })
        .with_context(|| {
            format!(
                "no retained local CI run before `{}` matches hook `{}` and its executed steps; pass an explicit base run",
                head.id, head.report.hook
            )
        })
}

type StepIdentity<'a> = (&'a str, &'a str, Option<&'a str>);

fn executed_steps(report: &LocalCiReport) -> Vec<StepIdentity<'_>> {
    report
        .steps
        .iter()
        .filter(|step| step.status != LocalCiStatus::Skipped)
        .map(|step| {
            (
                step.name.as_str(),
                step.command.as_str(),
                step.working_directory.as_deref(),
            )
        })
        .collect()
}

fn history_entry(id: &str, report: &LocalCiReport) -> LocalCiHistoryEntry {
    LocalCiHistoryEntry {
        id: id.to_owned(),
        hook: report.hook.clone(),
        started_at_unix_ms: report.started_at_unix_ms,
        duration_ms: report.duration_ms,
        status: report.status,
        steps: report.steps.len(),
    }
}

fn compare_runs(base: &StoredRun, head: &StoredRun) -> LocalCiComparison {
    let mut used_head = vec![false; head.report.steps.len()];
    let mut steps = Vec::new();
    for base_step in &base.report.steps {
        let matching = head
            .report
            .steps
            .iter()
            .enumerate()
            .find(|(index, head_step)| {
                !used_head[*index]
                    && head_step.name == base_step.name
                    && head_step.command == base_step.command
                    && head_step.working_directory == base_step.working_directory
            });
        if let Some((index, head_step)) = matching {
            used_head[index] = true;
            steps.push(step_comparison(Some(base_step), Some(head_step)));
        } else {
            steps.push(step_comparison(Some(base_step), None));
        }
    }
    for (index, head_step) in head.report.steps.iter().enumerate() {
        if !used_head[index] {
            steps.push(step_comparison(None, Some(head_step)));
        }
    }
    let duration_delta_ms = signed_delta(base.report.duration_ms, head.report.duration_ms);
    LocalCiComparison {
        schema_version: REPORT_SCHEMA_VERSION,
        base: history_entry(&base.id, &base.report),
        head: history_entry(&head.id, &head.report),
        duration_delta_ms,
        duration_delta_percent: (base.report.duration_ms > 0)
            .then(|| duration_delta_ms as f64 / base.report.duration_ms as f64 * 100.0),
        steps,
    }
}

fn step_comparison(
    base: Option<&LocalCiStepResult>,
    head: Option<&LocalCiStepResult>,
) -> LocalCiStepComparison {
    let reference = head.or(base).expect("a comparison has at least one step");
    LocalCiStepComparison {
        name: reference.name.clone(),
        command: reference.command.clone(),
        base_duration_ms: base.map(|step| step.duration_ms),
        head_duration_ms: head.map(|step| step.duration_ms),
        // A skipped step records 0 ms without running, so it measures nothing.
        duration_delta_ms: base
            .zip(head)
            .filter(|(base, head)| {
                base.status != LocalCiStatus::Skipped && head.status != LocalCiStatus::Skipped
            })
            .map(|(base, head)| signed_delta(base.duration_ms, head.duration_ms)),
        base_status: base.map(|step| step.status),
        head_status: head.map(|step| step.status),
    }
}

fn signed_delta(base: u128, head: u128) -> i128 {
    if head >= base {
        i128::try_from(head - base).unwrap_or(i128::MAX)
    } else {
        -i128::try_from(base - head).unwrap_or(i128::MAX)
    }
}

fn print_comparison_pretty(comparison: &LocalCiComparison) {
    println!("Local CI comparison ({})", comparison.head.hook);
    println!();
    println!(
        "  base  {}  {:.2}s  {}",
        comparison.base.id,
        comparison.base.duration_ms as f64 / 1000.0,
        status_label(comparison.base.status)
    );
    println!(
        "  head  {}  {:.2}s  {}",
        comparison.head.id,
        comparison.head.duration_ms as f64 / 1000.0,
        status_label(comparison.head.status)
    );
    let percent = comparison
        .duration_delta_percent
        .map(|value| format!(", {value:+.1}%"))
        .unwrap_or_default();
    println!(
        "  total change {:+.2}s{percent}",
        comparison.duration_delta_ms as f64 / 1000.0
    );
    println!();
    for step in &comparison.steps {
        match (
            step.base_duration_ms,
            step.head_duration_ms,
            step.duration_delta_ms,
        ) {
            (Some(base), Some(head), Some(delta)) => println!(
                "  {:<36} {:>7.2}s -> {:>7.2}s  ({:+.2}s)  {} -> {}",
                step.name,
                base as f64 / 1000.0,
                head as f64 / 1000.0,
                delta as f64 / 1000.0,
                step.base_status.map(status_label).unwrap_or("missing"),
                step.head_status.map(status_label).unwrap_or("missing")
            ),
            (Some(base), Some(head), None) => println!(
                "  {:<36} {:>8} -> {:>8}  (not compared)  {} -> {}",
                step.name,
                step_timing(base, step.base_status),
                step_timing(head, step.head_status),
                step.base_status.map(status_label).unwrap_or("missing"),
                step.head_status.map(status_label).unwrap_or("missing")
            ),
            (Some(base), None, None) => println!(
                "  {:<36} removed (was {:.2}s, {})",
                step.name,
                base as f64 / 1000.0,
                step.base_status.map(status_label).unwrap_or("missing")
            ),
            (None, Some(head), None) => {
                println!(
                    "  {:<36} added ({:.2}s, {})",
                    step.name,
                    head as f64 / 1000.0,
                    step.head_status.map(status_label).unwrap_or("missing")
                )
            }
            _ => {}
        }
    }
}

fn step_timing(duration_ms: u128, status: Option<LocalCiStatus>) -> String {
    if status == Some(LocalCiStatus::Skipped) {
        "skipped".to_owned()
    } else {
        format!("{:.2}s", duration_ms as f64 / 1000.0)
    }
}

fn status_label(status: LocalCiStatus) -> &'static str {
    match status {
        LocalCiStatus::Passed => "passed",
        LocalCiStatus::Failed => "failed",
        LocalCiStatus::Skipped => "skipped",
    }
}

/// Keeps the newest runs of each hook, so frequent hooks such as `commit-msg`
/// cannot evict the history of slower ones such as `pre-push`.
fn prune_history(directory: &Path) -> Result<()> {
    let mut runs = fs::read_dir(directory)
        .with_context(|| format!("could not read {}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| history_file_order(&path).map(|order| (order, path)))
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| right.cmp(left));
    let mut retained_by_hook = BTreeMap::<String, usize>::new();
    for (position, (_, path)) in runs.iter().enumerate() {
        let hook = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<RetainedHook>(&bytes)
                .ok()
                .map(|run| run.hook),
            // A concurrent prune already removed it.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => None,
        };
        let keep = match hook {
            Some(hook) => {
                let retained = retained_by_hook.entry(hook).or_default();
                *retained += 1;
                *retained <= RETAINED_RUNS
            }
            // Files without a readable hook keep the overall bound and cannot accumulate.
            None => position < RETAINED_RUNS,
        };
        if keep {
            continue;
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("could not prune {}", path.display()));
            }
        }
    }
    Ok(())
}

/// The only field pruning needs; later report fields are ignored.
#[derive(Deserialize)]
struct RetainedHook {
    hook: String,
}

fn history_file_order(path: &Path) -> Option<(u128, u16)> {
    let id = path
        .file_name()?
        .to_str()?
        .strip_prefix("run-")?
        .strip_suffix(".json")?;
    let (timestamp, suffix) = match id.split_once('-') {
        Some((timestamp, suffix)) => (timestamp, suffix.parse().ok()?),
        None => (id, 0),
    };
    Some((timestamp.parse().ok()?, suffix))
}

fn execute_step(
    root: &Path,
    hook_name: &str,
    number: usize,
    step: &HookStepConfig,
    hook_args: &[OsString],
    max_output_bytes: usize,
) -> LocalCiStepResult {
    let started = Instant::now();
    let directory = step
        .working_directory
        .as_ref()
        .map_or_else(|| root.to_path_buf(), |path| root.join(path));
    let passed_args = if step.pass_hook_args { hook_args } else { &[] };
    let display = command_display(step, &[]);
    let requires_hook_args = step.pass_hook_args && !hook_args.is_empty();
    let mut command = Command::new(&step.command);
    command
        .args(&step.args)
        .args(passed_args)
        .current_dir(&directory)
        .env("NO_COLOR", "1")
        .env("QUALITY_LOCAL_CI", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match command.spawn() {
        Ok(mut child) => {
            let stdout = child
                .stdout
                .take()
                .map(|reader| spawn_output_reader(reader, max_output_bytes));
            let stderr = child
                .stderr
                .take()
                .map(|reader| spawn_output_reader(reader, max_output_bytes));
            let status = child.wait();
            let stdout = collect_output(stdout);
            let stderr = collect_output(stderr);
            let (output, output_truncated) =
                combine_limited_output(&stdout, &stderr, max_output_bytes);
            match status {
                Ok(status) if status.success() => LocalCiStepResult {
                    number,
                    name: step_name(step),
                    status: LocalCiStatus::Passed,
                    duration_ms: started.elapsed().as_millis(),
                    command: display,
                    working_directory: relative_working_directory(step),
                    exit_code: status.code(),
                    output,
                    output_truncated,
                    diagnostics: Vec::new(),
                    failure: None,
                    rerun: None,
                },
                Ok(status) => {
                    let name = step_name(step);
                    let stdout_text =
                        String::from_utf8_lossy(retained_tail(&stdout, stdout.bytes.len()));
                    let stderr_text =
                        String::from_utf8_lossy(retained_tail(&stderr, stderr.bytes.len()));
                    let diagnostics = runner::parse_output(
                        step.parser,
                        &name,
                        root,
                        &directory,
                        &runner::ToolOutput {
                            stdout: &stdout_text,
                            stdout_capture: stdout.completeness(max_output_bytes),
                            stderr: &stderr_text,
                            combined: &output,
                        },
                        true,
                    )
                    .diagnostics;
                    LocalCiStepResult {
                        number,
                        name,
                        status: LocalCiStatus::Failed,
                        duration_ms: started.elapsed().as_millis(),
                        command: display,
                        working_directory: relative_working_directory(step),
                        exit_code: status.code(),
                        output,
                        output_truncated,
                        diagnostics,
                        failure: Some(match status.code() {
                            Some(code) => format!("command exited with code {code}"),
                            None => "command terminated by a signal".to_owned(),
                        }),
                        rerun: Some(rerun_command(hook_name, number, requires_hook_args)),
                    }
                }
                Err(error) => failed_step(
                    number,
                    step,
                    hook_name,
                    display,
                    started,
                    error,
                    requires_hook_args,
                ),
            }
        }
        Err(error) => failed_step(
            number,
            step,
            hook_name,
            display,
            started,
            error,
            requires_hook_args,
        ),
    }
}

fn failed_step(
    number: usize,
    step: &HookStepConfig,
    hook_name: &str,
    command: String,
    started: Instant,
    error: io::Error,
    requires_hook_args: bool,
) -> LocalCiStepResult {
    LocalCiStepResult {
        number,
        name: step_name(step),
        status: LocalCiStatus::Failed,
        duration_ms: started.elapsed().as_millis(),
        command,
        working_directory: relative_working_directory(step),
        exit_code: None,
        output: String::new(),
        output_truncated: false,
        diagnostics: vec![Diagnostic {
            tool: step_name(step),
            path: None,
            line: None,
            column: None,
            severity: "error".to_owned(),
            message: format!("could not run command: {error}"),
            rule: None,
        }],
        failure: Some(format!("could not run command: {error}")),
        rerun: Some(rerun_command(hook_name, number, requires_hook_args)),
    }
}

fn skipped_step(
    number: usize,
    step: &HookStepConfig,
    hook_name: &str,
    requires_hook_args: bool,
) -> LocalCiStepResult {
    LocalCiStepResult {
        number,
        name: step_name(step),
        status: LocalCiStatus::Skipped,
        duration_ms: 0,
        command: command_display(step, &[]),
        working_directory: relative_working_directory(step),
        exit_code: None,
        output: String::new(),
        output_truncated: false,
        diagnostics: Vec::new(),
        failure: None,
        rerun: Some(rerun_command(hook_name, number, requires_hook_args)),
    }
}

fn rerun_command(hook_name: &str, number: usize, requires_hook_args: bool) -> String {
    let suffix = if requires_hook_args {
        " -- <git-hook-args>"
    } else {
        ""
    };
    format!("quality ci local --hook {hook_name} --step {number}{suffix}")
}

fn relative_working_directory(step: &HookStepConfig) -> Option<String> {
    step.working_directory
        .as_ref()
        .map(|path| path.display().to_string())
}

fn step_name(step: &HookStepConfig) -> String {
    step.name
        .clone()
        .unwrap_or_else(|| command_display(step, &[]))
}

fn command_display(step: &HookStepConfig, hook_args: &[OsString]) -> String {
    std::iter::once(step.command.as_os_str())
        .chain(step.args.iter().map(OsStr::new))
        .chain(hook_args.iter().map(OsString::as_os_str))
        .map(display_argument)
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_argument(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._/:=@".contains(character))
    {
        text.into_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    }
}

/// The most recent bytes of one stream. A ring buffer discards the oldest
/// bytes in place, so retaining a large tail never shifts the whole capture.
#[derive(Clone, Default)]
struct TailCapture {
    tail: VecDeque<u8>,
    dropped: bool,
    starts_mid_line: bool,
    unfinished: bool,
}

impl TailCapture {
    fn push(&mut self, bytes: &[u8], limit: usize) {
        let retained = &bytes[bytes.len().saturating_sub(limit)..];
        let excess = (self.tail.len() + retained.len()).saturating_sub(limit);
        let last_dropped = if retained.len() < bytes.len() {
            Some(bytes[bytes.len() - retained.len() - 1])
        } else {
            excess
                .checked_sub(1)
                .and_then(|index| self.tail.get(index).copied())
        };
        if let Some(byte) = last_dropped {
            self.dropped = true;
            self.starts_mid_line = byte != b'\n';
        }
        self.tail.drain(..excess);
        self.tail.extend(retained);
    }
}

/// A collected stream. `dropped` means earlier bytes were discarded, and
/// `starts_mid_line` that the first retained line is therefore partial;
/// `unfinished` means later output may be missing because reading failed or
/// the stream stayed open.
#[derive(Default)]
struct CapturedOutput {
    bytes: Vec<u8>,
    dropped: bool,
    starts_mid_line: bool,
    unfinished: bool,
}

impl CapturedOutput {
    fn truncated(&self) -> bool {
        self.dropped || self.unfinished
    }

    fn completeness(&self, limit: usize) -> runner::StreamCapture {
        if self.dropped {
            runner::StreamCapture::Truncated(limit)
        } else if self.unfinished {
            runner::StreamCapture::Unfinished
        } else {
            runner::StreamCapture::Complete
        }
    }
}

struct OutputReader {
    state: Arc<Mutex<TailCapture>>,
    completed: mpsc::Receiver<()>,
}

fn spawn_output_reader(mut reader: impl Read + Send + 'static, limit: usize) -> OutputReader {
    let state = Arc::new(Mutex::new(TailCapture::default()));
    let thread_state = Arc::clone(&state);
    let (completed_tx, completed) = mpsc::channel();
    thread::spawn(move || {
        read_limited(&mut reader, limit, &thread_state);
        let _ = completed_tx.send(());
    });
    OutputReader { state, completed }
}

fn read_limited(reader: &mut impl Read, limit: usize, state: &Mutex<TailCapture>) {
    let mut buffer = [0_u8; 8192];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                if let Ok(mut captured) = state.lock() {
                    captured.unfinished = true;
                }
                break;
            }
        };
        if let Ok(mut captured) = state.lock() {
            captured.push(&buffer[..read], limit);
        }
    }
}

fn collect_output(reader: Option<OutputReader>) -> CapturedOutput {
    let Some(reader) = reader else {
        return CapturedOutput::default();
    };
    let completed = reader
        .completed
        .recv_timeout(Duration::from_millis(250))
        .is_ok();
    let Ok(mut state) = reader.state.lock() else {
        return CapturedOutput {
            unfinished: true,
            ..CapturedOutput::default()
        };
    };
    // A finished reader no longer touches its state; an unfinished one may.
    let captured = if completed {
        std::mem::take(&mut *state)
    } else {
        state.clone()
    };
    CapturedOutput {
        bytes: Vec::from(captured.tail),
        dropped: captured.dropped,
        starts_mid_line: captured.starts_mid_line,
        unfinished: captured.unfinished || !completed,
    }
}

/// Keeps the tail of both streams, reserving up to half of the limit for
/// standard error, and joins them without merging their boundary lines.
fn combine_limited_output(
    stdout: &CapturedOutput,
    stderr: &CapturedOutput,
    limit: usize,
) -> (String, bool) {
    let stderr_target = if stderr.bytes.is_empty() {
        0
    } else {
        limit.div_ceil(2)
    };
    let mut stderr_keep = stderr.bytes.len().min(stderr_target);
    let stdout_keep = stdout.bytes.len().min(limit.saturating_sub(stderr_keep));
    stderr_keep += stderr
        .bytes
        .len()
        .saturating_sub(stderr_keep)
        .min(limit.saturating_sub(stdout_keep + stderr_keep));
    (
        runner::join_streams(
            retained_tail(stdout, stdout_keep),
            retained_tail(stderr, stderr_keep),
        ),
        stdout.truncated()
            || stderr.truncated()
            || stdout_keep < stdout.bytes.len()
            || stderr_keep < stderr.bytes.len(),
    )
}

/// The last `keep` bytes of a stream. When the tail starts inside a line, that
/// partial line is dropped so its suffix cannot be parsed as a different path.
fn retained_tail(captured: &CapturedOutput, keep: usize) -> &[u8] {
    let start = captured.bytes.len() - keep;
    let tail = &captured.bytes[start..];
    let starts_mid_line = match start.checked_sub(1) {
        Some(previous) => captured.bytes[previous] != b'\n',
        None => captured.starts_mid_line,
    };
    if !starts_mid_line {
        return tail;
    }
    match tail.iter().position(|byte| *byte == b'\n') {
        Some(newline) if newline + 1 < tail.len() => &tail[newline + 1..],
        _ => tail,
    }
}

fn inspect_pull_request_workflows(
    root: &Path,
    local_steps: &[HookStepConfig],
) -> Result<Vec<WorkflowStepPlan>> {
    let directory = root.join(".github/workflows");
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(&directory)
        .with_context(|| format!("could not read {}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(OsStr::to_str),
                Some("yml" | "yaml")
            )
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut results = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let value: yaml_serde::Value = yaml_serde::from_str(&text)
            .with_context(|| format!("invalid workflow in {}", path.display()))?;
        if !has_pull_request_trigger(&value) {
            continue;
        }
        let workflow_env = mapping_value(&value, "env");
        let workflow_defaults = run_defaults(&value);
        let workflow = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("workflow")
            .to_owned();
        let Some(jobs) = mapping_value(&value, "jobs").and_then(yaml_serde::Value::as_mapping)
        else {
            continue;
        };
        for (job_id, job) in jobs {
            let job_name = job_id.as_str().unwrap_or("job").to_owned();
            if let Some(reusable) = mapping_value(job, "uses").and_then(yaml_serde::Value::as_str) {
                results.push(WorkflowStepPlan {
                    workflow: workflow.clone(),
                    job: job_name,
                    name: "Reusable workflow".to_owned(),
                    status: WorkflowStepStatus::GithubOnly,
                    detail: format!("uses {reusable}"),
                });
                continue;
            }
            let Some(steps) = mapping_value(job, "steps").and_then(yaml_serde::Value::as_sequence)
            else {
                continue;
            };
            let job_context = github_only_job_reason(job);
            let job_env = mapping_value(job, "env");
            let job_defaults = run_defaults(job);
            for (step_index, step) in steps.iter().enumerate() {
                let name = mapping_value(step, "name")
                    .and_then(yaml_serde::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Step {}", step_index + 1));
                if let Some(action) =
                    mapping_value(step, "uses").and_then(yaml_serde::Value::as_str)
                {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status: WorkflowStepStatus::GithubOnly,
                        detail: format!("GitHub-hosted action: {action}"),
                    });
                    continue;
                }
                let Some(run) = mapping_value(step, "run").and_then(yaml_serde::Value::as_str)
                else {
                    continue;
                };
                let conditional = mapping_value(step, "if").is_some_and(condition_requires_github);
                if conditional || run.contains("${{") {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status: WorkflowStepStatus::GithubOnly,
                        detail: "requires GitHub context or condition".to_owned(),
                    });
                    continue;
                }
                if let Some(reason) = job_context {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status: WorkflowStepStatus::GithubOnly,
                        detail: reason.to_owned(),
                    });
                    continue;
                }
                let step_env = mapping_value(step, "env");
                let shell = mapping_value(step, "shell")
                    .and_then(yaml_serde::Value::as_str)
                    .or_else(|| defaults_value(job_defaults, "shell"))
                    .or_else(|| defaults_value(workflow_defaults, "shell"));
                if let Some((status, detail)) =
                    execution_context_reason(workflow_env, job_env, step_env, shell)
                {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status,
                        detail,
                    });
                    continue;
                }
                if environment_setup_command(run) {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status: WorkflowStepStatus::GithubOnly,
                        detail: "hosted-runner environment setup".to_owned(),
                    });
                    continue;
                }
                let workflow_directory = mapping_value(step, "working-directory")
                    .and_then(yaml_serde::Value::as_str)
                    .or_else(|| defaults_value(job_defaults, "working-directory"))
                    .or_else(|| defaults_value(workflow_defaults, "working-directory"));
                if workflow_directory.is_some_and(|directory| directory.contains("${{")) {
                    results.push(WorkflowStepPlan {
                        workflow: workflow.clone(),
                        job: job_name.clone(),
                        name,
                        status: WorkflowStepStatus::GithubOnly,
                        detail: "working directory requires GitHub context".to_owned(),
                    });
                    continue;
                }
                let covered = local_steps.iter().any(|local| {
                    (normalize_command(run) == normalize_command(&command_display(local, &[]))
                        || local
                            .covers
                            .iter()
                            .any(|command| normalize_command(run) == normalize_command(command)))
                        && local
                            .working_directory
                            .as_ref()
                            .and_then(|path| path.to_str())
                            == workflow_directory
                });
                results.push(WorkflowStepPlan {
                    workflow: workflow.clone(),
                    job: job_name.clone(),
                    name,
                    status: if covered {
                        WorkflowStepStatus::Covered
                    } else {
                        WorkflowStepStatus::Uncovered
                    },
                    detail: if covered {
                        "matches the local gate".to_owned()
                    } else {
                        "localizable command is not in the selected hook".to_owned()
                    },
                });
            }
        }
    }
    Ok(results)
}

fn run_defaults(value: &yaml_serde::Value) -> Option<&yaml_serde::Value> {
    mapping_value(value, "defaults").and_then(|defaults| mapping_value(defaults, "run"))
}

fn defaults_value<'a>(defaults: Option<&'a yaml_serde::Value>, key: &str) -> Option<&'a str> {
    defaults
        .and_then(|value| mapping_value(value, key))
        .and_then(yaml_serde::Value::as_str)
}

fn execution_context_reason(
    workflow_env: Option<&yaml_serde::Value>,
    job_env: Option<&yaml_serde::Value>,
    step_env: Option<&yaml_serde::Value>,
    shell: Option<&str>,
) -> Option<(WorkflowStepStatus, String)> {
    let environments = [workflow_env, job_env, step_env]
        .into_iter()
        .flatten()
        .filter(|value| context_value_present(value))
        .collect::<Vec<_>>();
    if !environments.is_empty() {
        let github_context = environments.iter().any(|value| contains_expression(value));
        return Some((
            if github_context {
                WorkflowStepStatus::GithubOnly
            } else {
                WorkflowStepStatus::Uncovered
            },
            if github_context {
                "environment requires GitHub context".to_owned()
            } else {
                "workflow environment is not represented by the local hook".to_owned()
            },
        ));
    }
    shell.map(|shell| {
        if shell.contains("${{") {
            (
                WorkflowStepStatus::GithubOnly,
                "shell requires GitHub context".to_owned(),
            )
        } else {
            (
                WorkflowStepStatus::Uncovered,
                "workflow shell is not represented by the local hook".to_owned(),
            )
        }
    })
}

fn context_value_present(value: &yaml_serde::Value) -> bool {
    match value {
        yaml_serde::Value::Null => false,
        yaml_serde::Value::String(text) => !text.is_empty(),
        yaml_serde::Value::Sequence(values) => !values.is_empty(),
        yaml_serde::Value::Mapping(values) => !values.is_empty(),
        yaml_serde::Value::Tagged(tagged) => context_value_present(&tagged.value),
        _ => true,
    }
}

fn condition_requires_github(value: &yaml_serde::Value) -> bool {
    match value {
        yaml_serde::Value::Bool(true) => false,
        yaml_serde::Value::String(condition) => !matches!(condition.trim(), "true" | "${{ true }}"),
        yaml_serde::Value::Tagged(tagged) => condition_requires_github(&tagged.value),
        _ => true,
    }
}

fn contains_expression(value: &yaml_serde::Value) -> bool {
    match value {
        yaml_serde::Value::String(text) => text.contains("${{"),
        yaml_serde::Value::Sequence(values) => values.iter().any(contains_expression),
        yaml_serde::Value::Mapping(values) => values
            .iter()
            .any(|(key, value)| contains_expression(key) || contains_expression(value)),
        yaml_serde::Value::Tagged(tagged) => contains_expression(&tagged.value),
        _ => false,
    }
}

fn github_only_job_reason(job: &yaml_serde::Value) -> Option<&'static str> {
    if mapping_value(job, "strategy").is_some() {
        return Some("job uses a GitHub matrix strategy");
    }
    if mapping_value(job, "services").is_some() {
        return Some("job uses GitHub service containers");
    }
    if mapping_value(job, "container").is_some() {
        return Some("job uses a GitHub job container");
    }
    if mapping_value(job, "if").is_some_and(condition_requires_github)
        || mapping_value(job, "runs-on").is_some_and(contains_expression)
    {
        return Some("job requires GitHub context or conditions");
    }
    if let Some(runner) = mapping_value(job, "runs-on") {
        match runner_operating_system_value(runner) {
            Some(runner_os) if runner_os != std::env::consts::OS => {
                return Some("job uses a different runner operating system");
            }
            None => return Some("job runner operating system cannot be determined"),
            Some(_) => {}
        }
    }
    None
}

fn runner_operating_system_value(value: &yaml_serde::Value) -> Option<&'static str> {
    match value {
        yaml_serde::Value::String(label) => runner_operating_system(label),
        yaml_serde::Value::Sequence(labels) => {
            labels.iter().find_map(runner_operating_system_value)
        }
        yaml_serde::Value::Mapping(values) => values.iter().find_map(|(key, value)| {
            (key.as_str() == Some("labels"))
                .then(|| runner_operating_system_value(value))
                .flatten()
        }),
        yaml_serde::Value::Tagged(tagged) => runner_operating_system_value(&tagged.value),
        _ => None,
    }
}

fn runner_operating_system(label: &str) -> Option<&'static str> {
    let label = label.to_ascii_lowercase();
    if label == "linux" || label == "ubuntu" || label.starts_with("ubuntu-") {
        Some("linux")
    } else if label == "macos" || label.starts_with("macos-") {
        Some("macos")
    } else if label == "windows" || label.starts_with("windows-") {
        Some("windows")
    } else {
        None
    }
}

fn environment_setup_command(command: &str) -> bool {
    let normalized = normalize_command(command);
    if normalized
        .chars()
        .any(|character| matches!(character, '&' | '|' | ';' | '`'))
        || normalized.contains("$(")
    {
        return false;
    }
    normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .all(setup_command_fragment)
}

fn setup_command_fragment(command: &str) -> bool {
    [
        "pnpm install",
        "npm ci",
        "npm install",
        "yarn install",
        "bun install",
        "brew install",
        "rustup component add",
    ]
    .iter()
    .any(|prefix| command_starts_with_invocation(command, prefix))
        || go_tool_install_command(command)
        || writes_github_environment_file(command)
}

fn command_starts_with_invocation(command: &str, invocation: &str) -> bool {
    command
        .strip_prefix(invocation)
        .is_some_and(|remainder| remainder.is_empty() || remainder.starts_with(char::is_whitespace))
}

fn go_tool_install_command(command: &str) -> bool {
    command_starts_with_invocation(command, "go install")
        .then(|| command.strip_prefix("go install"))
        .flatten()
        .and_then(|arguments| arguments.split_ascii_whitespace().next())
        .is_some_and(|target| target.contains('@') && !target.starts_with('.'))
}

fn writes_github_environment_file(command: &str) -> bool {
    [
        "$GITHUB_PATH",
        "$GITHUB_ENV",
        "$env:GITHUB_PATH",
        "$env:GITHUB_ENV",
    ]
    .iter()
    .any(|variable| has_shell_redirection_to(command, variable))
}

fn has_shell_redirection_to(command: &str, variable: &str) -> bool {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    for (index, character) in command.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if !single_quoted => escaped = true,
            '\'' if !double_quoted => single_quoted = !single_quoted,
            '"' if !single_quoted => double_quoted = !double_quoted,
            '>' if !single_quoted
                && !double_quoted
                && redirection_target_matches(&command[index + 1..], variable) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn redirection_target_matches(remainder: &str, variable: &str) -> bool {
    let remainder = remainder
        .strip_prefix('>')
        .unwrap_or(remainder)
        .trim_start();
    if let Some(quoted) = remainder.strip_prefix('"') {
        return quoted
            .strip_prefix(variable)
            .is_some_and(|suffix| suffix.starts_with('"'));
    }
    if remainder.starts_with('\'') {
        return false;
    }
    remainder.strip_prefix(variable).is_some_and(|suffix| {
        suffix.is_empty()
            || suffix
                .chars()
                .next()
                .is_some_and(|character| character.is_whitespace() || ";&|".contains(character))
    })
}

fn has_pull_request_trigger(value: &yaml_serde::Value) -> bool {
    let Some(trigger) = mapping_value(value, "on") else {
        return false;
    };
    match trigger {
        yaml_serde::Value::String(name) => is_pull_request_event(name),
        yaml_serde::Value::Sequence(names) => names
            .iter()
            .filter_map(yaml_serde::Value::as_str)
            .any(is_pull_request_event),
        yaml_serde::Value::Mapping(events) => events
            .keys()
            .filter_map(yaml_serde::Value::as_str)
            .any(is_pull_request_event),
        _ => false,
    }
}

fn is_pull_request_event(event: &str) -> bool {
    matches!(event, "pull_request" | "pull_request_target")
}

fn mapping_value<'a>(value: &'a yaml_serde::Value, key: &str) -> Option<&'a yaml_serde::Value> {
    value
        .as_mapping()?
        .get(yaml_serde::Value::String(key.to_owned()))
}

fn normalize_command(command: &str) -> String {
    command.replace("\r\n", "\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_workflows_preserve_on_aliases_tags_and_expressions() {
        let workflow: yaml_serde::Value = yaml_serde::from_str(
            r#"on: [pull_request, workflow_dispatch]
env: &environment
  FLAG: 'on'
  VALUE: !context '${{ github.ref }}'
jobs:
  test:
    env: *environment
    runs-on: !runner [self-hosted, linux]
    steps:
      - run: |-
          pnpm check
          pnpm test
"#,
        )
        .unwrap();
        assert!(has_pull_request_trigger(&workflow));
        let job = mapping_value(mapping_value(&workflow, "jobs").unwrap(), "test").unwrap();
        let environment = mapping_value(job, "env").unwrap();
        assert!(contains_expression(environment));
        assert_eq!(
            mapping_value(environment, "FLAG").unwrap().as_str(),
            Some("on")
        );
        assert_eq!(
            runner_operating_system_value(mapping_value(job, "runs-on").unwrap()),
            Some("linux")
        );
        let steps = mapping_value(job, "steps").unwrap().as_sequence().unwrap();
        assert_eq!(
            mapping_value(&steps[0], "run").unwrap().as_str(),
            Some("pnpm check\npnpm test")
        );
    }

    #[test]
    fn identifies_hosted_runner_setup_commands() {
        assert!(environment_setup_command("pnpm install --frozen-lockfile"));
        assert!(environment_setup_command(
            "go install example.test/tool@v1\necho bin >> $GITHUB_PATH"
        ));
        assert!(!environment_setup_command(
            "pnpm install --frozen-lockfile && pnpm test"
        ));
        assert!(!environment_setup_command(
            "pnpm install --frozen-lockfile & pnpm test"
        ));
        assert!(!environment_setup_command("pnpm install\npnpm test"));
        assert!(!environment_setup_command("pnpm run check"));
        assert!(!environment_setup_command("go install ./..."));
        assert!(!environment_setup_command("npm install-test"));
        assert!(!environment_setup_command(
            "grep '$GITHUB_ENV' scripts/setup.sh"
        ));
        assert!(!environment_setup_command(
            "grep '>> $GITHUB_ENV' scripts/setup.sh"
        ));
        assert!(environment_setup_command("echo bin >> \"$GITHUB_PATH\""));
        assert!(!environment_setup_command("echo bin >> '$GITHUB_PATH'"));
    }

    #[test]
    fn retained_history_keeps_only_the_latest_twenty_runs() {
        let temporary = tempfile::tempdir().unwrap();
        for number in 0..22 {
            fs::write(temporary.path().join(format!("run-{number:02}.json")), "{}").unwrap();
        }
        fs::write(temporary.path().join("latest.json"), "{}").unwrap();

        prune_history(temporary.path()).unwrap();

        let mut names = fs::read_dir(temporary.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names.len(), 21);
        assert!(!names.contains(&"run-00.json".to_owned()));
        assert!(!names.contains(&"run-01.json".to_owned()));
        assert!(names.contains(&"run-21.json".to_owned()));
        assert!(names.contains(&"latest.json".to_owned()));
    }

    #[test]
    fn retained_history_does_not_replace_runs_started_in_the_same_millisecond() {
        let temporary = tempfile::tempdir().unwrap();
        let first = write_history_report(temporary.path(), &history_report(123, 1)).unwrap();
        let second = write_history_report(temporary.path(), &history_report(123, 2)).unwrap();

        assert_eq!(first.file_name().unwrap(), "run-123.json");
        assert_eq!(second.file_name().unwrap(), "run-123-0001.json");
        let original: LocalCiReport = serde_json::from_slice(&fs::read(first).unwrap()).unwrap();
        assert_eq!(original.duration_ms, 1);
    }

    #[test]
    fn concurrent_history_writes_preserve_every_complete_report() {
        let temporary = tempfile::tempdir().unwrap();
        write_history_report(temporary.path(), &history_report(123, 0)).unwrap();
        let barrier = std::sync::Barrier::new(8);

        let paths = thread::scope(|scope| {
            let handles = (1..=8)
                .map(|duration| {
                    let directory = temporary.path();
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        write_history_report(directory, &history_report(123, duration)).unwrap()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        let unique = paths.into_iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), 8);
        let runs = read_history_in(temporary.path(), None).unwrap();
        let mut durations = runs
            .iter()
            .map(|run| run.report.duration_ms)
            .collect::<Vec<_>>();
        durations.sort();
        assert_eq!(durations, (0..=8).collect::<Vec<_>>());
        assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 9);
    }

    #[test]
    fn history_selectors_order_equal_timestamps_and_never_resolve_paths() {
        let temporary = tempfile::tempdir().unwrap();
        for name in ["run-123-0002.json", "run-123.json", "run-123-0001.json"] {
            write_report(&history_report(123, 1), &temporary.path().join(name)).unwrap();
        }

        let runs = read_history_in(temporary.path(), None).unwrap();
        assert_eq!(
            select_history_run(&runs, "latest").unwrap().id,
            "run-123-0002"
        );
        assert_eq!(
            select_history_run(&runs, "previous").unwrap().id,
            "run-123-0001"
        );
        for selector in ["123", "run-123", "123.json", "run-123.json"] {
            assert_eq!(select_history_run(&runs, selector).unwrap().id, "run-123");
        }
        for selector in [
            "../run-123.json",
            "./run-123",
            "/run-123.json",
            "..\\run-123.json",
        ] {
            assert!(select_history_run(&runs, selector).is_err());
        }
    }

    #[test]
    fn history_pruning_orders_collisions_and_timestamp_widths_numerically() {
        let temporary = tempfile::tempdir().unwrap();
        write_history_report(temporary.path(), &history_report(9, 0)).unwrap();
        for duration in 0..=20 {
            write_history_report(temporary.path(), &history_report(10, duration)).unwrap();
        }

        prune_history(temporary.path()).unwrap();

        assert!(!temporary.path().join("run-9.json").exists());
        assert!(!temporary.path().join("run-10.json").exists());
        let runs = read_history_in(temporary.path(), None).unwrap();
        assert_eq!(runs.len(), RETAINED_RUNS);
        assert_eq!(
            select_history_run(&runs, "latest").unwrap().id,
            "run-10-0020"
        );
        assert_eq!(
            select_history_run(&runs, "previous").unwrap().id,
            "run-10-0019"
        );
    }

    #[test]
    fn comparison_default_base_precedes_the_selected_head_and_matches_its_hook() {
        let temporary = tempfile::tempdir().unwrap();
        for timestamp in 1..=4 {
            let mut report = history_report(timestamp, timestamp);
            if timestamp == 2 {
                report.hook = "pre-push".to_owned();
            }
            write_history_report(temporary.path(), &report).unwrap();
        }
        let runs = read_history_in(temporary.path(), None).unwrap();

        let latest = select_history_run(&runs, "latest").unwrap();
        assert_eq!(previous_history_run(&runs, latest).unwrap().id, "run-3");
        let older = select_history_run(&runs, "run-3").unwrap();
        assert_eq!(previous_history_run(&runs, older).unwrap().id, "run-1");
        let oldest = select_history_run(&runs, "run-1").unwrap();
        assert!(previous_history_run(&runs, oldest).is_err());
        let isolated_hook = select_history_run(&runs, "run-2").unwrap();
        assert!(previous_history_run(&runs, isolated_hook).is_err());
    }

    #[test]
    fn history_retention_is_counted_separately_for_each_hook() {
        let temporary = tempfile::tempdir().unwrap();
        for timestamp in [1, 2] {
            let mut report = history_report(timestamp, 1);
            report.hook = "pre-push".to_owned();
            write_history_report(temporary.path(), &report).unwrap();
        }
        for timestamp in 10..32 {
            let mut report = history_report(timestamp, 1);
            report.hook = "commit-msg".to_owned();
            write_history_report(temporary.path(), &report).unwrap();
        }
        // An unreadable file keeps the overall bound instead of accumulating.
        fs::write(temporary.path().join("run-0.json"), "not json").unwrap();

        prune_history(temporary.path()).unwrap();

        let commit_runs = read_history_in(temporary.path(), Some("commit-msg")).unwrap();
        assert_eq!(commit_runs.len(), RETAINED_RUNS);
        assert_eq!(commit_runs.last().unwrap().id, "run-12");
        let push_runs = read_history_in(temporary.path(), Some("pre-push")).unwrap();
        assert_eq!(
            push_runs
                .iter()
                .map(|run| run.id.as_str())
                .collect::<Vec<_>>(),
            ["run-2", "run-1"]
        );
        assert!(!temporary.path().join("run-0.json").exists());
    }

    fn step(
        number: usize,
        name: &str,
        status: LocalCiStatus,
        duration_ms: u128,
    ) -> LocalCiStepResult {
        LocalCiStepResult {
            number,
            name: name.to_owned(),
            status,
            duration_ms,
            command: name.to_ascii_lowercase(),
            working_directory: None,
            exit_code: None,
            output: String::new(),
            output_truncated: false,
            diagnostics: Vec::new(),
            failure: None,
            rerun: None,
        }
    }

    fn stored(id: &str, steps: Vec<LocalCiStepResult>) -> StoredRun {
        let mut report = history_report(0, steps.iter().map(|step| step.duration_ms).sum());
        report.steps = steps;
        StoredRun {
            id: id.to_owned(),
            report,
        }
    }

    #[test]
    fn comparisons_match_executed_steps_even_when_the_first_step_fails() {
        use LocalCiStatus::{Failed, Passed, Skipped};

        let whole = stored(
            "whole",
            vec![step(1, "Build", Passed, 5), step(2, "Test", Passed, 9)],
        );
        assert_eq!(
            executed_steps(&whole.report),
            vec![("Build", "build", None), ("Test", "test", None)]
        );
        // A failed first step may be either a focused run or a fail-fast run.
        // Both executed only Build and must not match a completed whole hook.
        let failed = stored(
            "failed",
            vec![step(1, "Build", Failed, 5), step(2, "Test", Skipped, 0)],
        );
        assert_eq!(
            executed_steps(&failed.report),
            vec![("Build", "build", None)]
        );
        let focused = stored(
            "focused",
            vec![step(1, "Build", Skipped, 0), step(2, "Test", Failed, 9)],
        );
        assert_eq!(
            executed_steps(&focused.report),
            vec![("Test", "test", None)]
        );
        let first_only = stored(
            "first",
            vec![step(1, "Build", Passed, 5), step(2, "Test", Skipped, 0)],
        );
        assert_eq!(
            executed_steps(&first_only.report),
            vec![("Build", "build", None)]
        );

        let runs = [
            stored(
                "run-5",
                vec![step(1, "Build", Skipped, 0), step(2, "Test", Passed, 7)],
            ),
            stored(
                "run-4",
                vec![step(1, "Build", Passed, 4), step(2, "Test", Skipped, 0)],
            ),
            stored(
                "run-3",
                vec![step(1, "Build", Passed, 4), step(2, "Test", Passed, 8)],
            ),
            stored(
                "run-2",
                vec![step(1, "Build", Skipped, 0), step(2, "Test", Passed, 6)],
            ),
        ];
        assert_eq!(previous_history_run(&runs, &runs[0]).unwrap().id, "run-2");
        assert!(previous_history_run(&runs, &runs[1]).is_err());
        assert!(previous_history_run(&runs, &runs[2]).is_err());
    }

    #[test]
    fn skipped_steps_have_no_duration_change() {
        use LocalCiStatus::{Passed, Skipped};

        let base = stored(
            "run-1",
            vec![step(1, "Build", Passed, 40), step(2, "Test", Passed, 10)],
        );
        let head = stored(
            "run-2",
            vec![step(1, "Build", Skipped, 0), step(2, "Test", Passed, 15)],
        );

        let comparison = compare_runs(&base, &head);

        assert_eq!(comparison.steps[0].base_duration_ms, Some(40));
        assert_eq!(comparison.steps[0].head_duration_ms, Some(0));
        assert_eq!(comparison.steps[0].duration_delta_ms, None);
        assert_eq!(comparison.steps[1].duration_delta_ms, Some(5));
    }

    fn history_report(started_at_unix_ms: u128, duration_ms: u128) -> LocalCiReport {
        LocalCiReport {
            schema_version: REPORT_SCHEMA_VERSION,
            hook: "pre-commit".to_owned(),
            started_at_unix_ms,
            duration_ms,
            status: LocalCiStatus::Passed,
            steps: Vec::new(),
        }
    }

    fn captured(bytes: &[u8], dropped_mid_line: bool) -> CapturedOutput {
        CapturedOutput {
            bytes: bytes.to_vec(),
            dropped: dropped_mid_line,
            starts_mid_line: dropped_mid_line,
            unfinished: false,
        }
    }

    #[test]
    fn bounded_output_keeps_the_failure_tail_and_reserves_stderr() {
        let (output, truncated) = combine_limited_output(
            &captured(b"verbose stdout that ends with OUT_TAIL", true),
            &captured(b"diagnostic context and FINAL_ERROR", true),
            24,
        );

        assert!(truncated);
        assert!(output.contains("FINAL_ERROR"));
        assert!(output.contains("OUT_TAIL"));
        assert!(!output.contains("verbose stdout"));
        assert!(
            output.contains("OUT_TAIL\n"),
            "streams stay on separate lines: {output:?}"
        );
    }

    #[test]
    fn bounded_output_drops_partial_leading_lines() {
        let (output, truncated) = combine_limited_output(
            &captured(b"rst.ts:1:1: error: cut\nsrc/b.ts:2:1: error: kept\n", true),
            &CapturedOutput::default(),
            1024,
        );
        assert!(truncated);
        assert_eq!(output, "src/b.ts:2:1: error: kept\n");

        // A single partial line is still better context than nothing.
        let (output, _) =
            combine_limited_output(&captured(b"l-rule)\n", true), &CapturedOutput::default(), 8);
        assert_eq!(output, "l-rule)\n");

        // Complete streams are never trimmed.
        let (output, truncated) = combine_limited_output(
            &captured(b"first\nsecond\n", false),
            &captured(b"error\n", false),
            1024,
        );
        assert!(!truncated);
        assert_eq!(output, "first\nsecond\nerror\n");
    }

    /// Yields a long stream in tiny reads, like a chatty command writing lines.
    struct ChunkedReader {
        remaining: usize,
        chunk: usize,
        next: u8,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.remaining.min(self.chunk).min(buffer.len());
            for byte in &mut buffer[..count] {
                *byte = self.next;
                self.next = self.next.wrapping_add(1);
            }
            self.remaining -= count;
            Ok(count)
        }
    }

    #[test]
    fn output_capture_retains_the_exact_tail_across_many_small_reads() {
        let total = 300_007;
        let limit = 4_096;
        let state = Mutex::new(TailCapture::default());
        let mut reader = ChunkedReader {
            remaining: total,
            chunk: 7,
            next: 0,
        };

        read_limited(&mut reader, limit, &state);

        let captured = state.into_inner().unwrap();
        assert!(captured.dropped);
        assert!(!captured.unfinished);
        let expected = (total - limit..total)
            .map(|index| (index % 256) as u8)
            .collect::<Vec<_>>();
        assert_eq!(Vec::from(captured.tail), expected);

        let state = Mutex::new(TailCapture::default());
        read_limited(
            &mut ChunkedReader {
                remaining: limit,
                chunk: 9_000,
                next: 0,
            },
            limit,
            &state,
        );
        let captured = state.into_inner().unwrap();
        assert!(!captured.dropped, "output exactly at the limit is complete");
        assert_eq!(captured.tail.len(), limit);
    }

    #[test]
    fn output_capture_keeps_only_the_tail_of_an_oversized_read() {
        let mut capture = TailCapture::default();
        capture.push(b"0123", 6);
        capture.push(b"long-suffix", 6);
        assert!(capture.dropped);
        assert!(capture.starts_mid_line);
        assert_eq!(Vec::from(capture.tail), b"suffix");
    }

    #[test]
    fn whole_lines_survive_a_cut_on_a_line_boundary() {
        let mut capture = TailCapture::default();
        capture.push(b"src/a.ts:1:1: error: old\n", 64);
        capture.push(b"src/b.ts:2:1: error: kept\n", 26);
        assert!(capture.dropped);
        assert!(!capture.starts_mid_line);
        let stream = CapturedOutput {
            bytes: Vec::from(capture.tail),
            dropped: true,
            starts_mid_line: false,
            unfinished: false,
        };
        let (output, truncated) = combine_limited_output(&stream, &CapturedOutput::default(), 64);
        assert!(truncated);
        assert_eq!(output, "src/b.ts:2:1: error: kept\n");

        // A cut made while combining also keeps a line that starts at the cut.
        let (output, _) = combine_limited_output(
            &captured(b"first\nsecond\n", false),
            &CapturedOutput::default(),
            7,
        );
        assert_eq!(output, "second\n");
    }

    #[test]
    fn rerun_marks_required_git_hook_arguments_without_persisting_them() {
        assert_eq!(
            rerun_command("commit-msg", 1, true),
            "quality ci local --hook commit-msg --step 1 -- <git-hook-args>"
        );
    }

    #[test]
    fn command_normalization_preserves_semantic_whitespace() {
        assert_ne!(
            normalize_command(r#"printf "a  b""#),
            normalize_command(r#"printf "a b""#)
        );
        assert_eq!(normalize_command("pnpm run check\r\n"), "pnpm run check");
    }

    #[test]
    fn rendered_arguments_preserve_literal_shell_semantics() {
        assert_eq!(display_argument(OsStr::new("$TOKEN")), "'$TOKEN'");
        assert_eq!(display_argument(OsStr::new("")), "''");
        assert_eq!(display_argument(OsStr::new("it's")), "'it'\"'\"'s'");
        assert_eq!(display_argument(OsStr::new("pnpm")), "pnpm");
    }

    #[test]
    fn runner_labels_map_to_their_operating_system() {
        assert_eq!(runner_operating_system("ubuntu-22.04"), Some("linux"));
        assert_eq!(runner_operating_system("macos-latest"), Some("macos"));
        assert_eq!(runner_operating_system("windows-2022"), Some("windows"));
        assert_eq!(runner_operating_system("self-hosted"), None);
        let labels =
            yaml_serde::from_str::<yaml_serde::Value>("[self-hosted, windows, x64]").unwrap();
        assert_eq!(runner_operating_system_value(&labels), Some("windows"));
        let expression =
            yaml_serde::from_str::<yaml_serde::Value>(r#"[self-hosted, "${{ vars.RUNNER_OS }}"]"#)
                .unwrap();
        assert!(contains_expression(&expression));
        let mapped = yaml_serde::from_str::<yaml_serde::Value>(
            "{ group: hosted, labels: [self-hosted, windows-2022] }",
        )
        .unwrap();
        assert_eq!(runner_operating_system_value(&mapped), Some("windows"));
        assert_eq!(
            runner_operating_system_value(&yaml_serde::Value::String("self-hosted".to_owned())),
            None
        );
        let self_hosted_job = yaml_serde::from_str::<yaml_serde::Value>(
            "runs-on: self-hosted\nsteps:\n  - run: pnpm test\n",
        )
        .unwrap();
        assert_eq!(
            github_only_job_reason(&self_hosted_job),
            Some("job runner operating system cannot be determined")
        );
    }

    #[test]
    fn literal_true_conditions_do_not_require_github_context() {
        assert!(!condition_requires_github(&yaml_serde::Value::Bool(true)));
        assert!(!condition_requires_github(&yaml_serde::Value::String(
            "${{ true }}".to_owned()
        )));
        assert!(condition_requires_github(&yaml_serde::Value::Bool(false)));
        assert!(condition_requires_github(&yaml_serde::Value::String(
            "github.ref == 'refs/heads/main'".to_owned()
        )));
    }
}
