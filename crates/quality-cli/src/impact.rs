//! Deterministic, committed-source impact plans. Execution and caches stay with
//! repository commands; result records are bindings, not signed attestations.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::cli::{CiOutputFormat, ImpactCommand, ImpactComparison};

const SCHEMA_VERSION: u8 = 1;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ImpactConfig {
    schema_version: u8,
    #[serde(default)]
    global_inputs: Vec<String>,
    #[serde(default)]
    ignored_inputs: Vec<String>,
    #[serde(deserialize_with = "deserialize_nodes")]
    nodes: BTreeMap<String, ImpactNode>,
}

fn deserialize_nodes<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, ImpactNode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Nodes;
    impl<'de> serde::de::Visitor<'de> for Nodes {
        type Value = BTreeMap<String, ImpactNode>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object containing unique impact node IDs")
        }

        fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut nodes = BTreeMap::new();
            while let Some((id, node)) = map.next_entry::<String, ImpactNode>()? {
                if nodes.insert(id.clone(), node).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate impact node ID `{id}`"
                    )));
                }
            }
            Ok(nodes)
        }
    }
    deserializer.deserialize_map(Nodes)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ImpactNode {
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    always: bool,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ImpactPlan {
    schema_version: u8,
    quality_version: String,
    plan_id: String,
    config_path: String,
    config: ImpactConfig,
    source: PlanSource,
    full_requested: bool,
    mode: PlanMode,
    changed_paths: Vec<String>,
    reasons: Vec<FullReason>,
    selected: Vec<PlannedNode>,
    skipped: Vec<SkippedNode>,
    batches: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PlanSource {
    requested_base: Option<String>,
    requested_head: String,
    comparison: ImpactComparison,
    base: Option<String>,
    head: Option<String>,
    merge_base: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PlanMode {
    Affected,
    Full,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum FullReason {
    ExplicitFull,
    HeadUnavailable,
    BaseUnavailable,
    MergeBaseUnavailable,
    ComparisonUnavailable,
    PolicyChanged,
    GlobalInput { path: String },
    UnclassifiedInput { path: String },
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionReason {
    Full,
    Always,
    Input { path: String },
    Dependent { node: String },
    Prerequisite { node: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PlannedNode {
    id: String,
    command: String,
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
    depends_on: Vec<String>,
    reasons: Vec<SelectionReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SkippedNode {
    id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpactResults {
    schema_version: u8,
    plan_id: String,
    head: String,
    results: Vec<NodeResult>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeResult {
    id: String,
    status: ResultStatus,
    exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ResultStatus {
    Passed,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Debug, Serialize)]
struct VerificationReport {
    schema_version: u8,
    plan_id: String,
    head: String,
    passed: bool,
    checked_nodes: usize,
    failures: Vec<VerificationFailure>,
}

#[derive(Debug, Serialize)]
struct VerificationFailure {
    id: String,
    reason: &'static str,
}

pub fn run(root: &Path, command: ImpactCommand) -> Result<()> {
    match command {
        ImpactCommand::Plan {
            config,
            base,
            head,
            comparison,
            full,
            format,
        } => {
            let (config_path, config) = load_config(root, &config)?;
            let plan = create_plan(
                root,
                config_path,
                config,
                base.as_deref(),
                &head,
                comparison,
                full,
            )?;
            match format {
                CiOutputFormat::Json => println!("{}", serde_json::to_string_pretty(&plan)?),
                CiOutputFormat::Pretty => print_plan(&plan),
            }
        }
        ImpactCommand::Verify {
            plan,
            results,
            format,
        } => {
            let plan_path = root.join(plan);
            let results_path = root.join(results);
            let plan: ImpactPlan = read_json(&plan_path)?;
            let results: ImpactResults = read_json(&results_path)?;
            ensure_clean_source(root, &[&plan_path, &results_path])?;
            let report = verify(root, &plan, &results)?;
            match format {
                CiOutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
                CiOutputFormat::Pretty => {
                    println!(
                        "Impact gate {}: {} planned nodes at {}",
                        if report.passed { "passed" } else { "failed" },
                        report.checked_nodes,
                        report.head
                    );
                    for failure in &report.failures {
                        println!("  {}: {}", failure.id, failure.reason);
                    }
                }
            }
            if !report.passed {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= MAX_JSON_BYTES,
        "{} exceeds the 16 MiB JSON limit",
        path.display()
    );
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn load_config(root: &Path, path: &Path) -> Result<(String, ImpactConfig)> {
    let root = root.canonicalize().context("project root does not exist")?;
    let path = root
        .join(path)
        .canonicalize()
        .context("impact configuration does not exist")?;
    let relative = path
        .strip_prefix(&root)
        .context("impact configuration must be inside the project root")?;
    let relative = relative
        .to_str()
        .context("impact configuration path must be UTF-8")?
        .replace(std::path::MAIN_SEPARATOR, "/");
    validate_path(&relative, false)?;
    let mut config: ImpactConfig = read_json(&path)?;
    config.normalize();
    config.validate()?;
    Ok((relative, config))
}

impl ImpactConfig {
    fn normalize(&mut self) {
        sort_unique(&mut self.global_inputs);
        sort_unique(&mut self.ignored_inputs);
        for node in self.nodes.values_mut() {
            sort_unique(&mut node.inputs);
            sort_unique(&mut node.depends_on);
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unsupported impact configuration schema_version; expected 1"
        );
        ensure!(
            !self.nodes.is_empty(),
            "impact configuration must declare at least one node"
        );
        for input in self.global_inputs.iter().chain(&self.ignored_inputs) {
            validate_path(input, true)?;
        }
        for (id, node) in &self.nodes {
            ensure!(
                !id.is_empty()
                    && id.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')),
                "invalid impact node ID `{id}`; use lowercase letters, digits, hyphens, or underscores"
            );
            ensure!(
                !node.command.trim().is_empty() && !node.command.chars().any(char::is_control),
                "impact node `{id}` must define a non-empty command without control characters"
            );
            ensure!(
                node.args.iter().all(|arg| !arg.contains('\0')),
                "impact node `{id}` has an argument containing NUL"
            );
            if let Some(directory) = &node.working_directory {
                validate_path(directory.trim_end_matches('/'), false)
                    .with_context(|| format!("invalid working_directory for impact node `{id}`"))?;
            }
            for input in &node.inputs {
                validate_path(input, true)?;
            }
            for dependency in &node.depends_on {
                ensure!(
                    self.nodes.contains_key(dependency),
                    "impact node `{id}` depends on unknown node `{dependency}`"
                );
                ensure!(id != dependency, "impact node `{id}` depends on itself");
            }
        }
        batches(self, &self.nodes.keys().cloned().collect())?;
        Ok(())
    }
}

fn sort_unique(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn validate_path(path: &str, directory_prefix: bool) -> Result<()> {
    let value = if directory_prefix {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    ensure!(
        !value.is_empty()
            && !value.chars().any(char::is_control)
            && !value.contains(['\\', '*', '?', '[', ']', ':'])
            && value
                .split('/')
                .all(|part| !matches!(part, "" | "." | "..")),
        "invalid impact path `{path}`; use a relative exact file or a directory prefix ending in /, without globs or parent traversal"
    );
    Ok(())
}

fn matches_input(input: &str, path: &str) -> bool {
    if input.ends_with('/') {
        path.starts_with(input)
    } else {
        input == path
    }
}

fn create_plan(
    root: &Path,
    config_path: String,
    config: ImpactConfig,
    base: Option<&str>,
    head: &str,
    comparison: ImpactComparison,
    full: bool,
) -> Result<ImpactPlan> {
    config.validate()?;
    let (source, changed_paths, mut reasons) = discover(root, base, head, comparison, full);
    let mut selections: BTreeMap<String, BTreeSet<SelectionReason>> = BTreeMap::new();
    for (id, node) in &config.nodes {
        if node.always {
            selections
                .entry(id.clone())
                .or_default()
                .insert(SelectionReason::Always);
        }
    }
    for path in &changed_paths {
        if path == &config_path {
            reasons.insert(FullReason::PolicyChanged);
        }
        let global = config
            .global_inputs
            .iter()
            .any(|input| matches_input(input, path));
        if global {
            reasons.insert(FullReason::GlobalInput { path: path.clone() });
        }
        let mut classified = false;
        for (id, node) in &config.nodes {
            if node.inputs.iter().any(|input| matches_input(input, path)) {
                selections
                    .entry(id.clone())
                    .or_default()
                    .insert(SelectionReason::Input { path: path.clone() });
                classified = true;
            }
        }
        if !classified
            && !global
            && path != &config_path
            && !config
                .ignored_inputs
                .iter()
                .any(|input| matches_input(input, path))
        {
            reasons.insert(FullReason::UnclassifiedInput { path: path.clone() });
        }
    }
    let mode = if reasons.is_empty() {
        PlanMode::Affected
    } else {
        PlanMode::Full
    };
    if mode == PlanMode::Full {
        for id in config.nodes.keys() {
            selections
                .entry(id.clone())
                .or_default()
                .insert(SelectionReason::Full);
        }
    } else {
        // First propagate actual impact downstream; prerequisite inclusion must
        // not invalidate otherwise unaffected siblings sharing the prerequisite.
        loop {
            let mut additions = Vec::new();
            for (id, node) in &config.nodes {
                for dependency in &node.depends_on {
                    if selections.contains_key(dependency) {
                        additions.push((
                            id.clone(),
                            SelectionReason::Dependent {
                                node: dependency.clone(),
                            },
                        ));
                    }
                }
            }
            let mut changed = false;
            for (id, reason) in additions {
                changed |= selections.entry(id).or_default().insert(reason);
            }
            if !changed {
                break;
            }
        }
        loop {
            let mut additions = Vec::new();
            for id in selections.keys() {
                for dependency in &config.nodes[id].depends_on {
                    additions.push((
                        dependency.clone(),
                        SelectionReason::Prerequisite { node: id.clone() },
                    ));
                }
            }
            let mut changed = false;
            for (id, reason) in additions {
                changed |= selections.entry(id).or_default().insert(reason);
            }
            if !changed {
                break;
            }
        }
    }
    let selected_ids = selections.keys().cloned().collect();
    let batches = batches(&config, &selected_ids)?;
    let selected = selections
        .into_iter()
        .map(|(id, reasons)| {
            let node = &config.nodes[&id];
            PlannedNode {
                id: id.clone(),
                command: node.command.clone(),
                args: node.args.clone(),
                working_directory: node.working_directory.clone(),
                depends_on: node.depends_on.clone(),
                reasons: reasons.into_iter().collect(),
            }
        })
        .collect();
    let skipped = config
        .nodes
        .keys()
        .filter(|id| !selected_ids.contains(*id))
        .map(|id| SkippedNode {
            id: id.clone(),
            reason: "No changed input, affected dependency, or selected dependent".to_owned(),
        })
        .collect();
    let mut plan = ImpactPlan {
        schema_version: SCHEMA_VERSION,
        quality_version: env!("CARGO_PKG_VERSION").to_owned(),
        plan_id: String::new(),
        config_path,
        config,
        source,
        full_requested: full,
        mode,
        changed_paths,
        reasons: reasons.into_iter().collect(),
        selected,
        skipped,
        batches,
    };
    plan.plan_id = plan_identity(root, &plan)?;
    Ok(plan)
}

fn batches(config: &ImpactConfig, selected: &BTreeSet<String>) -> Result<Vec<Vec<String>>> {
    let mut pending = selected.clone();
    let mut complete = BTreeSet::new();
    let mut result = Vec::new();
    while !pending.is_empty() {
        let ready: Vec<String> = pending
            .iter()
            .filter(|id| {
                config.nodes[*id]
                    .depends_on
                    .iter()
                    .all(|dependency| complete.contains(dependency))
            })
            .cloned()
            .collect();
        ensure!(
            !ready.is_empty(),
            "impact dependency graph contains a cycle or an omitted prerequisite"
        );
        for id in &ready {
            pending.remove(id);
            complete.insert(id.clone());
        }
        result.push(ready);
    }
    Ok(result)
}

fn discover(
    root: &Path,
    base: Option<&str>,
    head: &str,
    comparison: ImpactComparison,
    full: bool,
) -> (PlanSource, Vec<String>, BTreeSet<FullReason>) {
    let mut reasons = BTreeSet::new();
    if full {
        reasons.insert(FullReason::ExplicitFull);
    }
    let mut source = PlanSource {
        requested_base: base.map(str::to_owned),
        requested_head: head.to_owned(),
        comparison,
        base: None,
        head: None,
        merge_base: None,
    };
    source.head = resolve_revision(root, head).ok();
    if source.head.is_none() {
        reasons.insert(FullReason::HeadUnavailable);
    }
    if let Some(base) = base {
        source.base = resolve_revision(root, base).ok();
        if source.base.is_none() {
            reasons.insert(FullReason::BaseUnavailable);
        }
    } else if !full {
        reasons.insert(FullReason::BaseUnavailable);
    }
    let (Some(base), Some(head)) = (&source.base, &source.head) else {
        return (source, Vec::new(), reasons);
    };
    let ancestor = match comparison {
        ImpactComparison::Direct => base.clone(),
        ImpactComparison::MergeBase => match git_text(root, &["merge-base", base, head]) {
            Ok(ancestor) => {
                source.merge_base = Some(ancestor.clone());
                ancestor
            }
            Err(_) => {
                reasons.insert(FullReason::MergeBaseUnavailable);
                return (source, Vec::new(), reasons);
            }
        },
    };
    let paths = match changed_paths(root, &ancestor, head) {
        Ok(paths) => paths,
        Err(_) => {
            reasons.insert(FullReason::ComparisonUnavailable);
            Vec::new()
        }
    };
    (source, paths, reasons)
}

fn changed_paths(root: &Path, base: &str, head: &str) -> Result<Vec<String>> {
    let output = git(
        root,
        &[
            "diff",
            "--relative",
            "--name-only",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "-z",
            base,
            head,
            "--",
            ".",
        ],
    )?;
    ensure!(output.status.success(), "Git comparison failed");
    let mut paths = BTreeSet::new();
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8(path.to_vec()).context("Git returned a non-UTF-8 path")?;
        // Git paths may contain literal glob characters; only their relative
        // structure matters here, not the stricter configuration matcher syntax.
        ensure!(
            !path.starts_with('/') && path.split('/').all(|part| !matches!(part, "" | "." | "..")),
            "Git returned an invalid relative path"
        );
        paths.insert(path);
    }
    Ok(paths.into_iter().collect())
}

fn resolve_revision(root: &Path, revision: &str) -> Result<String> {
    let revision = format!("{revision}^{{commit}}");
    let value = git_text(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &revision,
        ],
    )?;
    ensure!(
        (value.len() == 40 || value.len() == 64)
            && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid commit identifier"
    );
    Ok(value)
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(key);
    }
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command
}

fn git(root: &Path, args: &[&str]) -> Result<Output> {
    git_command(root)
        .args(args)
        .output()
        .context("could not run Git")
}

fn git_text(root: &Path, args: &[&str]) -> Result<String> {
    let output = git(root, args)?;
    ensure!(output.status.success(), "Git command failed");
    Ok(String::from_utf8(output.stdout)
        .context("Git returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn plan_identity(root: &Path, plan: &ImpactPlan) -> Result<String> {
    let mut canonical = plan.clone();
    canonical.plan_id.clear();
    let bytes = serde_json::to_vec(&canonical)?;
    let mut child = git_command(root)
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Git is required to bind the impact plan identity")?;
    let mut stdin = child
        .stdin
        .take()
        .context("could not open Git hash input")?;
    stdin.write_all(&bytes)?;
    drop(stdin);
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Git could not hash the impact plan"
    );
    let digest = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        (digest.len() == 40 || digest.len() == 64)
            && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git returned an invalid plan digest"
    );
    Ok(format!("git-blob:{digest}"))
}

fn verify(root: &Path, plan: &ImpactPlan, results: &ImpactResults) -> Result<VerificationReport> {
    ensure!(
        plan.schema_version == SCHEMA_VERSION && results.schema_version == SCHEMA_VERSION,
        "unsupported impact plan or result schema_version; expected 1"
    );
    ensure!(
        plan.quality_version == env!("CARGO_PKG_VERSION"),
        "impact plan was produced by a different Quality version; regenerate it"
    );
    validate_path(&plan.config_path, false)?;
    plan.config.validate()?;
    ensure!(
        plan_identity(root, plan)? == plan.plan_id,
        "impact plan identity does not match its contents"
    );
    let head = plan
        .source
        .head
        .as_deref()
        .context("cannot verify a plan without a resolved source revision")?;
    ensure!(
        resolve_revision(root, "HEAD")? == head,
        "checkout HEAD does not match the impact plan revision"
    );
    let (config_path, config) = load_config(root, Path::new(&plan.config_path))?;
    ensure!(
        config == plan.config,
        "impact policy changed after planning; regenerate the plan"
    );
    // Verification requires the policy itself to belong to the claimed commit.
    let prefix = git_text(root, &["rev-parse", "--show-prefix"])?;
    let committed_policy = git(
        root,
        &["show", &format!("{head}:{prefix}{}", plan.config_path)],
    )?;
    ensure!(
        committed_policy.status.success(),
        "impact policy must be committed at the planned revision before verification"
    );
    let mut committed_config: ImpactConfig = serde_json::from_slice(&committed_policy.stdout)
        .context("committed impact policy is invalid")?;
    committed_config.normalize();
    ensure!(
        committed_config == config,
        "working impact policy differs from the planned source revision"
    );
    let base = plan
        .source
        .base
        .as_deref()
        .or(plan.source.requested_base.as_deref());
    let mut expected = create_plan(
        root,
        config_path,
        config,
        base,
        head,
        plan.source.comparison,
        plan.full_requested,
    )?;
    expected.source.requested_base = plan.source.requested_base.clone();
    expected.source.requested_head = plan.source.requested_head.clone();
    expected.plan_id = plan_identity(root, &expected)?;
    ensure!(
        &expected == plan,
        "impact plan selection, dependency batches, or Git comparison is inconsistent; regenerate it"
    );
    ensure!(
        results.plan_id == plan.plan_id,
        "results belong to a different impact plan"
    );
    ensure!(
        results.head == head,
        "results belong to a different source revision"
    );
    let selected: BTreeSet<_> = plan.selected.iter().map(|node| node.id.as_str()).collect();
    let mut records = BTreeMap::new();
    for result in &results.results {
        ensure!(
            selected.contains(result.id.as_str()),
            "result for unselected or unknown node `{}`",
            result.id
        );
        ensure!(
            records.insert(result.id.as_str(), result).is_none(),
            "duplicate result for impact node `{}`",
            result.id
        );
    }
    let mut failures = Vec::new();
    for id in &selected {
        let reason = match records.get(id) {
            None => Some("missing result"),
            Some(result) => match result.status {
                ResultStatus::Passed if result.exit_code == Some(0) => None,
                ResultStatus::Passed => Some("passed result requires exit_code 0"),
                ResultStatus::Failed => Some("node failed"),
                ResultStatus::Cancelled => Some("node was cancelled"),
                ResultStatus::Skipped => Some("selected node was skipped"),
            },
        };
        if let Some(reason) = reason {
            failures.push(VerificationFailure {
                id: (*id).to_owned(),
                reason,
            });
        }
    }
    Ok(VerificationReport {
        schema_version: SCHEMA_VERSION,
        plan_id: plan.plan_id.clone(),
        head: head.to_owned(),
        passed: failures.is_empty(),
        checked_nodes: selected.len(),
        failures,
    })
}

fn ensure_clean_source(root: &Path, artifacts: &[&Path]) -> Result<()> {
    // A normal diff trusts these index flags and can hide modified or missing
    // tracked inputs. Reject them without changing the caller's index.
    let tracked_paths = git(
        root,
        &["ls-files", "--recurse-submodules", "-v", "-z", "--", "."],
    )?;
    ensure!(
        tracked_paths.status.success(),
        "could not inspect tracked source index flags"
    );
    for entry in tracked_paths.stdout.split(|byte| *byte == 0) {
        let Some(tag) = entry.first() else {
            continue;
        };
        let path = String::from_utf8_lossy(entry.get(2..).unwrap_or_default());
        ensure!(
            !tag.is_ascii_lowercase() && *tag != b'S',
            "impact verification rejects assume-unchanged or skip-worktree index flags on {path:?}; clear those flags or use a fresh full checkout, then rerun verification"
        );
    }
    let tracked = git(
        root,
        &[
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
            "HEAD",
            "--",
            ".",
        ],
    )?;
    ensure!(
        tracked.status.success(),
        "impact verification requires a clean committed checkout; tracked files differ from HEAD"
    );
    let canonical_root = root.canonicalize()?;
    let mut allowed = BTreeSet::new();
    for artifact in artifacts {
        let canonical = artifact
            .canonicalize()
            .context("impact artifact does not exist")?;
        if let Ok(relative) = canonical.strip_prefix(&canonical_root) {
            allowed.insert(
                relative
                    .to_str()
                    .context("impact artifact path must be UTF-8")?
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
    }
    let output = git(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            ".",
        ],
    )?;
    ensure!(
        output.status.success(),
        "could not inspect untracked source files"
    );
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path =
            String::from_utf8(path.to_vec()).context("untracked source path must be UTF-8")?;
        ensure!(
            allowed.contains(&path),
            "impact verification requires a clean committed checkout; untracked file `{path}` is not a supplied plan or results artifact"
        );
    }
    Ok(())
}

fn print_plan(plan: &ImpactPlan) {
    println!(
        "Impact plan: {:?} ({} changed paths, {} selected, {} skipped)",
        plan.mode,
        plan.changed_paths.len(),
        plan.selected.len(),
        plan.skipped.len()
    );
    println!("Plan: {}", plan.plan_id);
    println!(
        "Source: {}",
        plan.source
            .head
            .as_deref()
            .unwrap_or("unavailable; verification will require a resolved revision")
    );
    println!("Only committed source changes are compared; commands are not executed.");
    for reason in &plan.reasons {
        println!("Full selection: {reason:?}");
    }
    for (index, batch) in plan.batches.iter().enumerate() {
        println!("Batch {}: {}", index + 1, batch.join(", "));
    }
    for node in &plan.selected {
        println!(
            "  {}: {} {:?} ({:?})",
            node.id, node.command, node.args, node.reasons
        );
    }
    for node in &plan.skipped {
        println!("Skip {}: {}", node.id, node.reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn checked_git(root: &Path, args: &[&str]) -> String {
        let output = git(root, args).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn write(root: &Path, path: &str, content: &str) {
        let target = root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, content).unwrap();
    }

    fn fixture_config() -> ImpactConfig {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "global_inputs": ["shared/", "lockfile"],
            "ignored_inputs": ["docs/"],
            "nodes": {
                "core": {"inputs":["core/"],"command":"cargo","args":["build"]},
                "web": {"inputs":["web/"],"depends_on":["core"],"command":"pnpm","args":["exec","turbo","run","build"]},
                "browser": {"depends_on":["web"],"command":"pnpm","args":["test:browser"]},
                "other": {"inputs":["other/"],"depends_on":["core"],"command":"cargo","args":["test"]},
                "native": {"inputs":["native/"],"command":"./gradlew","args":["test"]},
                "audit": {"always":true,"command":"pnpm","args":["audit"]}
            }
        })).unwrap()
    }

    fn repository() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        checked_git(temp.path(), &["init", "--quiet"]);
        checked_git(
            temp.path(),
            &["config", "user.email", "quality@example.test"],
        );
        checked_git(temp.path(), &["config", "user.name", "Quality Tests"]);
        write(
            temp.path(),
            "quality-impact.json",
            &serde_json::to_string_pretty(&fixture_config()).unwrap(),
        );
        for path in [
            "core/code.rs",
            "web/view.ts",
            "other/code.rs",
            "native/Main.kt",
            "docs/guide.md",
        ] {
            write(temp.path(), path, "initial\n");
        }
        commit(temp.path());
        temp
    }

    fn commit(root: &Path) {
        checked_git(root, &["add", "."]);
        checked_git(root, &["commit", "--quiet", "-m", "fixture"]);
    }

    fn plan(root: &Path, base: &str) -> ImpactPlan {
        let (path, config) = load_config(root, Path::new("quality-impact.json")).unwrap();
        create_plan(
            root,
            path,
            config,
            Some(base),
            "HEAD",
            ImpactComparison::MergeBase,
            false,
        )
        .unwrap()
    }

    fn ids(plan: &ImpactPlan) -> Vec<&str> {
        plan.selected.iter().map(|node| node.id.as_str()).collect()
    }

    fn passing_results(plan: &ImpactPlan) -> ImpactResults {
        ImpactResults {
            schema_version: 1,
            plan_id: plan.plan_id.clone(),
            head: plan.source.head.clone().unwrap(),
            results: plan
                .selected
                .iter()
                .map(|node| NodeResult {
                    id: node.id.clone(),
                    status: ResultStatus::Passed,
                    exit_code: Some(0),
                })
                .collect(),
        }
    }

    #[test]
    fn selects_dependents_and_prerequisites_without_unaffected_siblings() {
        let repo = repository();
        write(repo.path(), "web/view.ts", "changed\n");
        commit(repo.path());
        let plan = plan(repo.path(), "HEAD~1");
        assert_eq!(ids(&plan), ["audit", "browser", "core", "web"]);
        assert_eq!(plan.mode, PlanMode::Affected);
        assert_eq!(
            plan.batches,
            vec![vec!["audit", "core"], vec!["web"], vec!["browser"]]
        );
        assert!(
            verify(repo.path(), &plan, &passing_results(&plan))
                .unwrap()
                .passed
        );
    }

    #[test]
    fn propagates_shared_package_change_transitively() {
        let repo = repository();
        write(repo.path(), "core/code.rs", "changed\n");
        commit(repo.path());
        let plan = plan(repo.path(), "HEAD~1");
        assert_eq!(ids(&plan), ["audit", "browser", "core", "other", "web"]);
        assert!(!ids(&plan).contains(&"native"));
    }

    #[test]
    fn no_change_and_ignored_docs_keep_live_checks() {
        let repo = repository();
        assert_eq!(ids(&plan(repo.path(), "HEAD")), ["audit"]);
        write(repo.path(), "docs/guide.md", "changed\n");
        commit(repo.path());
        assert_eq!(ids(&plan(repo.path(), "HEAD~1")), ["audit"]);
    }

    #[test]
    fn shared_unknown_and_policy_changes_select_full_graph() {
        for path in ["shared/config", "unknown file", "lockfile"] {
            let repo = repository();
            write(repo.path(), path, "changed\n");
            commit(repo.path());
            let plan = plan(repo.path(), "HEAD~1");
            assert_eq!(plan.mode, PlanMode::Full, "{path}");
            assert_eq!(plan.selected.len(), fixture_config().nodes.len());
        }
        let repo = repository();
        let mut config = fixture_config();
        config.ignored_inputs.push("notes/".to_owned());
        write(
            repo.path(),
            "quality-impact.json",
            &serde_json::to_string(&config).unwrap(),
        );
        commit(repo.path());
        assert!(
            plan(repo.path(), "HEAD~1")
                .reasons
                .contains(&FullReason::PolicyChanged)
        );
    }

    #[test]
    fn renames_and_deletions_include_old_scope_and_unicode_paths() {
        let repo = repository();
        checked_git(repo.path(), &["mv", "web/view.ts", "native/á view.ts"]);
        fs::remove_file(repo.path().join("other/code.rs")).unwrap();
        commit(repo.path());
        let plan = plan(repo.path(), "HEAD~1");
        assert_eq!(
            plan.changed_paths,
            ["native/á view.ts", "other/code.rs", "web/view.ts"]
        );
        assert_eq!(plan.mode, PlanMode::Affected);
        assert_eq!(plan.selected.len(), fixture_config().nodes.len());
    }

    #[test]
    fn plans_are_deterministic_and_ignore_local_source_edits() {
        let repo = repository();
        let first = plan(repo.path(), "HEAD");
        write(repo.path(), "web/view.ts", "uncommitted\n");
        write(repo.path(), "new-untracked", "untracked\n");
        checked_git(repo.path(), &["add", "web/view.ts"]);
        let second = plan(repo.path(), "HEAD");
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
    }

    #[test]
    fn distinguishes_merge_base_and_direct_comparisons() {
        let repo = repository();
        let ancestor = resolve_revision(repo.path(), "HEAD").unwrap();
        write(repo.path(), "native/Main.kt", "base branch\n");
        commit(repo.path());
        let base = resolve_revision(repo.path(), "HEAD").unwrap();
        checked_git(repo.path(), &["checkout", "--quiet", "--detach", &ancestor]);
        write(repo.path(), "web/view.ts", "head branch\n");
        commit(repo.path());
        let merge = plan(repo.path(), &base);
        let (path, config) = load_config(repo.path(), Path::new("quality-impact.json")).unwrap();
        let direct = create_plan(
            repo.path(),
            path,
            config,
            Some(&base),
            "HEAD",
            ImpactComparison::Direct,
            false,
        )
        .unwrap();
        assert!(!ids(&merge).contains(&"native"));
        assert!(ids(&direct).contains(&"native"));
        assert!(
            verify(repo.path(), &direct, &passing_results(&direct))
                .unwrap()
                .passed
        );
    }

    #[test]
    fn missing_or_hostile_refs_fall_back_without_shell_interpretation() {
        let repo = repository();
        for base in [
            "missing-ref",
            "--help",
            "HEAD;touch injected",
            "$(touch injected)",
        ] {
            let plan = plan(repo.path(), base);
            assert_eq!(plan.mode, PlanMode::Full);
            assert!(plan.reasons.contains(&FullReason::BaseUnavailable));
            assert!(
                verify(repo.path(), &plan, &passing_results(&plan))
                    .unwrap()
                    .passed
            );
        }
        assert!(!repo.path().join("injected").exists());
    }

    #[test]
    fn full_plan_without_base_and_missing_head_are_explicit() {
        let repo = repository();
        let (path, config) = load_config(repo.path(), Path::new("quality-impact.json")).unwrap();
        let full = create_plan(
            repo.path(),
            path.clone(),
            config.clone(),
            None,
            "HEAD",
            ImpactComparison::MergeBase,
            true,
        )
        .unwrap();
        assert!(
            verify(repo.path(), &full, &passing_results(&full))
                .unwrap()
                .passed
        );
        let invalid = create_plan(
            repo.path(),
            path,
            config,
            None,
            "missing",
            ImpactComparison::MergeBase,
            true,
        )
        .unwrap();
        assert_eq!(invalid.mode, PlanMode::Full);
        let results = ImpactResults {
            schema_version: 1,
            plan_id: invalid.plan_id.clone(),
            head: String::new(),
            results: Vec::new(),
        };
        assert!(verify(repo.path(), &invalid, &results).is_err());
    }

    #[test]
    fn configuration_rejects_empty_unknown_cyclic_and_escaping_graphs() {
        assert!(serde_json::from_str::<ImpactConfig>(
            r#"{"schema_version":1,"nodes":{"check":{"command":"first"},"check":{"command":"second"}}}"#
        ).is_err());
        let mut empty = fixture_config();
        empty.nodes.clear();
        assert!(empty.validate().is_err());
        let mut missing = fixture_config();
        missing
            .nodes
            .get_mut("core")
            .unwrap()
            .depends_on
            .push("missing".to_owned());
        assert!(missing.validate().is_err());
        let mut cycle = fixture_config();
        cycle
            .nodes
            .get_mut("core")
            .unwrap()
            .depends_on
            .push("browser".to_owned());
        assert!(cycle.validate().is_err());
        let mut self_edge = fixture_config();
        self_edge
            .nodes
            .get_mut("core")
            .unwrap()
            .depends_on
            .push("core".to_owned());
        assert!(self_edge.validate().is_err());
        for path in [
            "../outside",
            "/absolute",
            "src/**",
            "./src",
            "src//",
            "C:/repo",
            "src\\file",
        ] {
            let mut config = fixture_config();
            config
                .nodes
                .get_mut("core")
                .unwrap()
                .inputs
                .push(path.to_owned());
            assert!(config.validate().is_err(), "{path}");
        }
        let mut cwd = fixture_config();
        cwd.nodes.get_mut("core").unwrap().working_directory = Some("../other".to_owned());
        assert!(cwd.validate().is_err());
        assert!(
            serde_json::from_str::<ImpactConfig>(
                r#"{"schema_version":1,"nodes":{},"unknown":true}"#
            )
            .is_err()
        );
    }

    #[test]
    fn verifier_rejects_missing_failed_skipped_cancelled_and_invalid_success() {
        let repo = repository();
        let plan = plan(repo.path(), "HEAD");
        let mut missing = passing_results(&plan);
        missing.results.clear();
        assert!(!verify(repo.path(), &plan, &missing).unwrap().passed);
        for status in [
            ResultStatus::Failed,
            ResultStatus::Cancelled,
            ResultStatus::Skipped,
        ] {
            let mut results = passing_results(&plan);
            results.results[0].status = status;
            assert!(!verify(repo.path(), &plan, &results).unwrap().passed);
        }
        for exit_code in [None, Some(1), Some(-1)] {
            let mut results = passing_results(&plan);
            results.results[0].exit_code = exit_code;
            assert!(!verify(repo.path(), &plan, &results).unwrap().passed);
        }
    }

    #[test]
    fn verifier_rejects_duplicate_unknown_stale_and_malformed_records() {
        let repo = repository();
        let plan = plan(repo.path(), "HEAD");
        let mut duplicate = passing_results(&plan);
        duplicate.results.push(NodeResult {
            id: "audit".to_owned(),
            status: ResultStatus::Passed,
            exit_code: Some(0),
        });
        assert!(verify(repo.path(), &plan, &duplicate).is_err());
        let mut unknown = passing_results(&plan);
        unknown.results[0].id = "native".to_owned();
        assert!(verify(repo.path(), &plan, &unknown).is_err());
        let mut stale = passing_results(&plan);
        stale.head = "old".to_owned();
        assert!(verify(repo.path(), &plan, &stale).is_err());
        let mut stale = passing_results(&plan);
        stale.plan_id = "old".to_owned();
        assert!(verify(repo.path(), &plan, &stale).is_err());
        assert!(serde_json::from_str::<ImpactResults>(r#"{"schema_version":1,"plan_id":"id","head":"sha","results":[{"id":"audit","status":"pending","exit_code":0}]}"#).is_err());
    }

    #[test]
    fn verifier_recomputes_structure_even_after_digest_is_rewritten() {
        let repo = repository();
        write(repo.path(), "web/view.ts", "changed\n");
        commit(repo.path());
        let original = plan(repo.path(), "HEAD~1");
        for mutation in 0..4 {
            let mut altered = original.clone();
            match mutation {
                0 => altered.selected.clear(),
                1 => altered.batches.reverse(),
                2 => altered.changed_paths.clear(),
                _ => altered.selected[0].command = "changed-command".to_owned(),
            }
            altered.plan_id = plan_identity(repo.path(), &altered).unwrap();
            assert!(verify(repo.path(), &altered, &passing_results(&altered)).is_err());
        }
    }

    #[test]
    fn verifier_rejects_new_head_and_uncommitted_policy() {
        let repo = repository();
        let original = plan(repo.path(), "HEAD");
        write(repo.path(), "docs/guide.md", "changed\n");
        commit(repo.path());
        assert!(verify(repo.path(), &original, &passing_results(&original)).is_err());
        let mut config = fixture_config();
        config.ignored_inputs.push("new-notes/".to_owned());
        write(
            repo.path(),
            "quality-impact.json",
            &serde_json::to_string(&config).unwrap(),
        );
        let local = plan(repo.path(), "HEAD");
        assert!(verify(repo.path(), &local, &passing_results(&local)).is_err());
    }

    #[test]
    fn verification_requires_clean_sources_but_allows_exact_untracked_artifacts() {
        let repo = repository();
        ensure_clean_source(repo.path(), &[]).unwrap();
        let plan_path = repo.path().join("plan.json");
        let results_path = repo.path().join("results.json");
        write(repo.path(), "plan.json", "{}");
        write(repo.path(), "results.json", "{}");
        assert!(ensure_clean_source(repo.path(), &[]).is_err());
        ensure_clean_source(repo.path(), &[&plan_path, &results_path]).unwrap();
        write(repo.path(), "web/new.ts", "untracked source\n");
        assert!(ensure_clean_source(repo.path(), &[&plan_path, &results_path]).is_err());
        fs::remove_file(repo.path().join("web/new.ts")).unwrap();
        write(repo.path(), "web/view.ts", "modified tracked source\n");
        assert!(ensure_clean_source(repo.path(), &[&plan_path, &results_path]).is_err());
        // Naming a tracked source as an artifact never exempts its modification.
        assert!(ensure_clean_source(repo.path(), &[&repo.path().join("web/view.ts")]).is_err());
    }

    #[test]
    fn verification_rejects_hidden_tracked_edits_without_mutating_index_flags() {
        for flag in ["--assume-unchanged", "--skip-worktree"] {
            let repo = repository();
            checked_git(repo.path(), &["update-index", flag, "web/view.ts"]);
            let before = checked_git(repo.path(), &["ls-files", "-v", "web/view.ts"]);
            write(repo.path(), "web/view.ts", "hidden modified source\n");
            assert!(
                git(repo.path(), &["diff", "--quiet", "HEAD"])
                    .unwrap()
                    .status
                    .success()
            );
            let error = ensure_clean_source(repo.path(), &[]).unwrap_err();
            assert!(error.to_string().contains("index flags"), "{error}");
            assert_eq!(
                checked_git(repo.path(), &["ls-files", "-v", "web/view.ts"]),
                before
            );
        }
    }

    #[test]
    fn verification_detects_dirty_submodules_even_when_git_config_ignores_them() {
        let repo = repository();
        let dependency = repository();
        checked_git(
            repo.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--quiet",
                dependency.path().to_str().unwrap(),
                "dependency",
            ],
        );
        commit(repo.path());
        checked_git(repo.path(), &["config", "diff.ignoreSubmodules", "all"]);
        ensure_clean_source(repo.path(), &[]).unwrap();
        write(
            repo.path(),
            "dependency/web/view.ts",
            "modified dependency source\n",
        );
        assert!(ensure_clean_source(repo.path(), &[]).is_err());
        checked_git(
            &repo.path().join("dependency"),
            &["update-index", "--assume-unchanged", "web/view.ts"],
        );
        let error = ensure_clean_source(repo.path(), &[]).unwrap_err();
        assert!(error.to_string().contains("index flags"), "{error}");
        checked_git(
            &repo.path().join("dependency"),
            &["update-index", "--no-assume-unchanged", "web/view.ts"],
        );
        checked_git(
            &repo.path().join("dependency"),
            &["checkout", "--", "web/view.ts"],
        );
        write(
            repo.path(),
            "dependency/new-source.ts",
            "untracked dependency source\n",
        );
        assert!(ensure_clean_source(repo.path(), &[]).is_err());
    }

    #[test]
    fn git_command_clears_repository_and_config_overrides() {
        let command = git_command(Path::new("."));
        let removed: BTreeSet<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_str().unwrap())
            .collect();
        assert!(removed.contains("GIT_DIR"));
        assert!(removed.contains("GIT_WORK_TREE"));
        assert!(removed.contains("GIT_INDEX_FILE"));
        assert!(removed.contains("GIT_CONFIG_COUNT"));
    }

    #[test]
    fn impact_cli_is_separate_from_existing_ci_plan() {
        assert!(
            crate::cli::Cli::try_parse_from([
                "quality", "impact", "plan", "--base", "main", "--format", "json"
            ])
            .is_ok()
        );
        assert!(crate::cli::Cli::try_parse_from(["quality", "impact", "plan", "--full"]).is_ok());
        assert!(crate::cli::Cli::try_parse_from(["quality", "impact", "plan"]).is_err());
        assert!(crate::cli::Cli::try_parse_from(["quality", "ci", "plan"]).is_ok());
    }
}
