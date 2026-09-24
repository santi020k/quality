use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::cli::CiOutputFormat;
use crate::config::{Config, HookStepConfig};

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

#[derive(Clone, Debug, Serialize)]
pub struct LocalCiReport {
    pub schema_version: u8,
    pub hook: String,
    pub started_at_unix_ms: u128,
    pub duration_ms: u128,
    pub status: LocalCiStatus,
    pub steps: Vec<LocalCiStepResult>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCiStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Serialize)]
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
    #[serde(skip_serializing_if = "String::is_empty")]
    pub output: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub output_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun: Option<String>,
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
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "quality/local-ci"])
        .current_dir(root)
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
        if let Some(configured) = configured_steps.get(index) {
            step.command = command_display(configured, &[]);
        }
    }
    let run_path = directory.join(format!("run-{}.json", report.started_at_unix_ms));
    write_report(&retained, &run_path)?;
    write_report(&retained, &directory.join("latest.json"))?;
    prune_history(&directory)?;
    Ok(Some(run_path))
}

fn prune_history(directory: &Path) -> Result<()> {
    let mut runs = fs::read_dir(directory)
        .with_context(|| format!("could not read {}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with("run-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    runs.sort();
    let remove_count = runs.len().saturating_sub(RETAINED_RUNS);
    for path in runs.into_iter().take(remove_count) {
        fs::remove_file(&path).with_context(|| format!("could not prune {}", path.display()))?;
    }
    Ok(())
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
                combine_limited_output(stdout, stderr, max_output_bytes);
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
                    failure: None,
                    rerun: None,
                },
                Ok(status) => LocalCiStepResult {
                    number,
                    name: step_name(step),
                    status: LocalCiStatus::Failed,
                    duration_ms: started.elapsed().as_millis(),
                    command: display,
                    working_directory: relative_working_directory(step),
                    exit_code: status.code(),
                    output,
                    output_truncated,
                    failure: Some(match status.code() {
                        Some(code) => format!("command exited with code {code}"),
                        None => "command terminated by a signal".to_owned(),
                    }),
                    rerun: Some(rerun_command(hook_name, number, requires_hook_args)),
                },
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

type CapturedOutput = (Vec<u8>, bool);

struct OutputReader {
    state: Arc<Mutex<CapturedOutput>>,
    completed: mpsc::Receiver<()>,
}

fn spawn_output_reader(mut reader: impl Read + Send + 'static, limit: usize) -> OutputReader {
    let state = Arc::new(Mutex::new((Vec::new(), false)));
    let thread_state = Arc::clone(&state);
    let (completed_tx, completed) = mpsc::channel();
    thread::spawn(move || {
        read_limited(&mut reader, limit, &thread_state);
        let _ = completed_tx.send(());
    });
    OutputReader { state, completed }
}

fn read_limited(reader: &mut impl Read, limit: usize, state: &Mutex<CapturedOutput>) {
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            if let Ok(mut captured) = state.lock() {
                captured.1 = true;
            }
            break;
        };
        if read == 0 {
            break;
        }
        if let Ok(mut captured) = state.lock() {
            captured.0.extend_from_slice(&buffer[..read]);
            if captured.0.len() > limit {
                let excess = captured.0.len() - limit;
                captured.0.drain(..excess);
                captured.1 = true;
            }
        }
    }
}

fn collect_output(reader: Option<OutputReader>) -> CapturedOutput {
    let Some(reader) = reader else {
        return (Vec::new(), false);
    };
    let completed = reader
        .completed
        .recv_timeout(Duration::from_millis(250))
        .is_ok();
    let mut captured = reader
        .state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_else(|_| (Vec::new(), true));
    captured.1 |= !completed;
    captured
}

fn combine_limited_output(
    stdout: CapturedOutput,
    stderr: CapturedOutput,
    limit: usize,
) -> (String, bool) {
    let stderr_target = if stderr.0.is_empty() {
        0
    } else {
        limit.div_ceil(2)
    };
    let mut stderr_keep = stderr.0.len().min(stderr_target);
    let stdout_keep = stdout.0.len().min(limit.saturating_sub(stderr_keep));
    stderr_keep += stderr
        .0
        .len()
        .saturating_sub(stderr_keep)
        .min(limit.saturating_sub(stdout_keep + stderr_keep));
    let mut bytes = stdout.0[stdout.0.len().saturating_sub(stdout_keep)..].to_vec();
    bytes.extend_from_slice(&stderr.0[stderr.0.len().saturating_sub(stderr_keep)..]);
    (
        String::from_utf8_lossy(&bytes).into_owned(),
        stdout.1 || stderr.1 || stdout_keep < stdout.0.len() || stderr_keep < stderr.0.len(),
    )
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
        let value: serde_yaml::Value = serde_yaml::from_str(&text)
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
        let Some(jobs) = mapping_value(&value, "jobs").and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for (job_id, job) in jobs {
            let job_name = job_id.as_str().unwrap_or("job").to_owned();
            if let Some(reusable) = mapping_value(job, "uses").and_then(serde_yaml::Value::as_str) {
                results.push(WorkflowStepPlan {
                    workflow: workflow.clone(),
                    job: job_name,
                    name: "Reusable workflow".to_owned(),
                    status: WorkflowStepStatus::GithubOnly,
                    detail: format!("uses {reusable}"),
                });
                continue;
            }
            let Some(steps) = mapping_value(job, "steps").and_then(serde_yaml::Value::as_sequence)
            else {
                continue;
            };
            let job_context = github_only_job_reason(job);
            let job_env = mapping_value(job, "env");
            let job_defaults = run_defaults(job);
            for (step_index, step) in steps.iter().enumerate() {
                let name = mapping_value(step, "name")
                    .and_then(serde_yaml::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Step {}", step_index + 1));
                if let Some(action) =
                    mapping_value(step, "uses").and_then(serde_yaml::Value::as_str)
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
                let Some(run) = mapping_value(step, "run").and_then(serde_yaml::Value::as_str)
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
                    .and_then(serde_yaml::Value::as_str)
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
                    .and_then(serde_yaml::Value::as_str)
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

fn run_defaults(value: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
    mapping_value(value, "defaults").and_then(|defaults| mapping_value(defaults, "run"))
}

fn defaults_value<'a>(defaults: Option<&'a serde_yaml::Value>, key: &str) -> Option<&'a str> {
    defaults
        .and_then(|value| mapping_value(value, key))
        .and_then(serde_yaml::Value::as_str)
}

fn execution_context_reason(
    workflow_env: Option<&serde_yaml::Value>,
    job_env: Option<&serde_yaml::Value>,
    step_env: Option<&serde_yaml::Value>,
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

fn context_value_present(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null => false,
        serde_yaml::Value::String(text) => !text.is_empty(),
        serde_yaml::Value::Sequence(values) => !values.is_empty(),
        serde_yaml::Value::Mapping(values) => !values.is_empty(),
        serde_yaml::Value::Tagged(tagged) => context_value_present(&tagged.value),
        _ => true,
    }
}

fn condition_requires_github(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Bool(true) => false,
        serde_yaml::Value::String(condition) => !matches!(condition.trim(), "true" | "${{ true }}"),
        serde_yaml::Value::Tagged(tagged) => condition_requires_github(&tagged.value),
        _ => true,
    }
}

fn contains_expression(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::String(text) => text.contains("${{"),
        serde_yaml::Value::Sequence(values) => values.iter().any(contains_expression),
        serde_yaml::Value::Mapping(values) => values
            .iter()
            .any(|(key, value)| contains_expression(key) || contains_expression(value)),
        serde_yaml::Value::Tagged(tagged) => contains_expression(&tagged.value),
        _ => false,
    }
}

fn github_only_job_reason(job: &serde_yaml::Value) -> Option<&'static str> {
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

fn runner_operating_system_value(value: &serde_yaml::Value) -> Option<&'static str> {
    match value {
        serde_yaml::Value::String(label) => runner_operating_system(label),
        serde_yaml::Value::Sequence(labels) => {
            labels.iter().find_map(runner_operating_system_value)
        }
        serde_yaml::Value::Mapping(values) => values.iter().find_map(|(key, value)| {
            (key.as_str() == Some("labels"))
                .then(|| runner_operating_system_value(value))
                .flatten()
        }),
        serde_yaml::Value::Tagged(tagged) => runner_operating_system_value(&tagged.value),
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

fn has_pull_request_trigger(value: &serde_yaml::Value) -> bool {
    let Some(trigger) = mapping_value(value, "on") else {
        return false;
    };
    match trigger {
        serde_yaml::Value::String(name) => is_pull_request_event(name),
        serde_yaml::Value::Sequence(names) => names
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .any(is_pull_request_event),
        serde_yaml::Value::Mapping(events) => events
            .keys()
            .filter_map(serde_yaml::Value::as_str)
            .any(is_pull_request_event),
        _ => false,
    }
}

fn is_pull_request_event(event: &str) -> bool {
    matches!(event, "pull_request" | "pull_request_target")
}

fn mapping_value<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value
        .as_mapping()?
        .get(serde_yaml::Value::String(key.to_owned()))
}

fn normalize_command(command: &str) -> String {
    command.replace("\r\n", "\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn bounded_output_keeps_the_failure_tail_and_reserves_stderr() {
        let (output, truncated) = combine_limited_output(
            (b"verbose stdout that ends with OUT_TAIL".to_vec(), true),
            (b"diagnostic context and FINAL_ERROR".to_vec(), true),
            24,
        );

        assert!(truncated);
        assert!(output.contains("FINAL_ERROR"));
        assert!(output.contains("OUT_TAIL"));
        assert!(!output.contains("verbose stdout"));
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
            serde_yaml::from_str::<serde_yaml::Value>("[self-hosted, windows, x64]").unwrap();
        assert_eq!(runner_operating_system_value(&labels), Some("windows"));
        let expression =
            serde_yaml::from_str::<serde_yaml::Value>(r#"[self-hosted, "${{ vars.RUNNER_OS }}"]"#)
                .unwrap();
        assert!(contains_expression(&expression));
        let mapped = serde_yaml::from_str::<serde_yaml::Value>(
            "{ group: hosted, labels: [self-hosted, windows-2022] }",
        )
        .unwrap();
        assert_eq!(runner_operating_system_value(&mapped), Some("windows"));
        assert_eq!(
            runner_operating_system_value(&serde_yaml::Value::String("self-hosted".to_owned())),
            None
        );
        let self_hosted_job = serde_yaml::from_str::<serde_yaml::Value>(
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
        assert!(!condition_requires_github(&serde_yaml::Value::Bool(true)));
        assert!(!condition_requires_github(&serde_yaml::Value::String(
            "${{ true }}".to_owned()
        )));
        assert!(condition_requires_github(&serde_yaml::Value::Bool(false)));
        assert!(condition_requires_github(&serde_yaml::Value::String(
            "github.ref == 'refs/heads/main'".to_owned()
        )));
    }
}
