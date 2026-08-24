use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::{GateProfile, OutputFormat};
use crate::project::Project;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub output: OutputFormat,
    pub baseline: PathBuf,
    pub tools: BTreeMap<String, ToolConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tasks: BTreeMap<String, TaskConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hooks: BTreeMap<String, HookConfig>,
    #[serde(rename = "custom", skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_tools: BTreeMap<String, ExternalToolConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            output: OutputFormat::Pretty,
            baseline: PathBuf::from(".quality-baseline.json"),
            tools: BTreeMap::new(),
            tasks: BTreeMap::new(),
            hooks: BTreeMap::new(),
            custom_tools: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Participate in `quality check`. Formatting and fixes remain available when false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<PathBuf>,
    /// Run this adapter from a repository-relative workspace directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_args: Option<Vec<String>>,
    /// Maximum runtime for each invocation of this adapter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticParser {
    #[default]
    Generic,
    Codespell,
    EslintJson,
    SwiftlintJson,
    KtlintJson,
    SantiOgJson,
    TyposJson,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalFileMode {
    #[default]
    Append,
    Project,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default = "enabled_by_default")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub file_mode: ExternalFileMode,
    #[serde(default)]
    pub parser: DiagnosticParser,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub check_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_check_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

/// A repository-defined quality gate. Tasks run for `quality check` only and
/// let projects preserve their canonical lint, type-check, test, or build command.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    #[serde(default = "enabled_by_default")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_files: Vec<String>,
    #[serde(default)]
    pub parser: DiagnosticParser,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

/// Version-controlled behavior for one Git hook event.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookConfig {
    pub steps: Vec<HookStepConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookStepConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub command: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    #[serde(default)]
    pub pass_hook_args: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl Config {
    pub fn load_or_default(root: &Path) -> Result<Self> {
        let path = root.join("quality.yml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&text)
            .with_context(|| format!("invalid configuration in {}", path.display()))?;
        if config.version != 1 {
            anyhow::bail!(
                "unsupported quality.yml version {}; expected 1",
                config.version
            );
        }
        config.validate()?;
        Ok(config)
    }

    pub fn output_format(&self) -> OutputFormat {
        self.output
    }

    pub fn tool(&self, id: &str) -> ToolConfig {
        self.tools.get(id).cloned().unwrap_or_default()
    }

    pub fn baseline_path(&self, root: &Path) -> PathBuf {
        if self.baseline.is_absolute() {
            self.baseline.clone()
        } else {
            root.join(&self.baseline)
        }
    }

    pub fn validate_adapter_selection(&self, ids: &[String]) -> Result<()> {
        let supported: Vec<_> = crate::tools::catalog()
            .into_iter()
            .map(|tool| tool.id.to_owned())
            .chain(self.tasks.keys().cloned())
            .chain(self.custom_tools.keys().cloned())
            .collect();
        for id in ids {
            if supported.contains(id) {
                continue;
            }
            let suggestion = supported
                .iter()
                .min_by_key(|candidate| edit_distance(id, candidate))
                .filter(|candidate| edit_distance(id, candidate) <= 3)
                .map(|candidate| format!(" Did you mean `{candidate}`?"))
                .unwrap_or_default();
            anyhow::bail!(
                "unknown adapter `{id}`.{suggestion} Available adapters: {}",
                supported.join(", ")
            );
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let supported: Vec<_> = crate::tools::catalog()
            .into_iter()
            .map(|tool| tool.id)
            .collect();
        for id in self.tools.keys() {
            if supported.contains(&id.as_str()) {
                continue;
            }
            let suggestion = supported
                .iter()
                .min_by_key(|candidate| edit_distance(id, candidate))
                .filter(|candidate| edit_distance(id, candidate) <= 3)
                .map(|candidate| format!(" Did you mean `{candidate}`?"))
                .unwrap_or_default();
            anyhow::bail!(
                "unknown tool `{id}` in quality.yml.{suggestion} Supported tools: {}",
                supported.join(", ")
            );
        }
        for (id, tool) in &self.tools {
            if let Some(directory) = &tool.working_directory {
                validate_working_directory(id, directory)?;
            }
            validate_timeout(id, tool.timeout_seconds)?;
        }
        for (id, task) in &self.tasks {
            if supported.contains(&id.as_str()) || self.custom_tools.contains_key(id) {
                anyhow::bail!("task `{id}` conflicts with another configured adapter");
            }
            validate_custom_id(id, "task")?;
            if task.command.as_os_str().is_empty() {
                anyhow::bail!("task `{id}` must define a non-empty command");
            }
            if let Some(directory) = &task.working_directory {
                validate_working_directory(id, directory)?;
            }
            validate_extensions(id, "task", &task.extensions)?;
            validate_timeout(id, task.timeout_seconds)?;
        }
        for (id, tool) in &self.custom_tools {
            if supported.contains(&id.as_str()) {
                anyhow::bail!(
                    "custom tool `{id}` conflicts with a built-in tool; configure the built-in under `tools` instead"
                );
            }
            validate_custom_id(id, "custom tool")?;
            if tool.command.as_os_str().is_empty() {
                anyhow::bail!("custom tool `{id}` must define a non-empty command");
            }
            if let Some(directory) = &tool.working_directory {
                validate_working_directory(id, directory)?;
            }
            validate_extensions(id, "custom tool", &tool.extensions)?;
            validate_timeout(id, tool.timeout_seconds)?;
        }
        for (event, hook) in &self.hooks {
            validate_hook_event(event)?;
            if hook.steps.is_empty() {
                anyhow::bail!("hook `{event}` must define at least one step");
            }
            for (index, step) in hook.steps.iter().enumerate() {
                if step.command.as_os_str().is_empty() {
                    anyhow::bail!(
                        "step {} in hook `{event}` must define a non-empty command",
                        index + 1
                    );
                }
                if let Some(directory) = &step.working_directory {
                    validate_working_directory(event, directory)?;
                }
            }
        }
        Ok(())
    }
}

fn validate_hook_event(event: &str) -> Result<()> {
    const EVENTS: &[&str] = &[
        "applypatch-msg",
        "commit-msg",
        "fsmonitor-watchman",
        "post-applypatch",
        "post-checkout",
        "post-commit",
        "post-merge",
        "post-receive",
        "post-rewrite",
        "post-update",
        "pre-applypatch",
        "pre-auto-gc",
        "pre-commit",
        "pre-merge-commit",
        "pre-push",
        "pre-rebase",
        "pre-receive",
        "prepare-commit-msg",
        "proc-receive",
        "push-to-checkout",
        "reference-transaction",
        "sendemail-validate",
        "update",
    ];
    if !EVENTS.contains(&event) {
        anyhow::bail!("unsupported Git hook `{event}`");
    }
    Ok(())
}

fn validate_custom_id(id: &str, kind: &str) -> Result<()> {
    if !valid_tool_id(id) {
        anyhow::bail!(
            "invalid {kind} ID `{id}`; use lowercase letters, numbers, hyphens, or underscores"
        );
    }
    Ok(())
}

fn validate_extensions(id: &str, kind: &str, extensions: &[String]) -> Result<()> {
    if let Some(extension) = extensions.iter().find(|value| {
        value.is_empty()
            || value.starts_with('.')
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    }) {
        anyhow::bail!(
            "invalid extension `{extension}` for {kind} `{id}`; write extensions without a leading dot"
        );
    }
    Ok(())
}

fn validate_working_directory(id: &str, directory: &Path) -> Result<()> {
    use std::path::Component;

    if directory.is_absolute()
        || directory
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        anyhow::bail!(
            "working directory for `{id}` must stay inside the repository and be relative"
        );
    }
    Ok(())
}

fn validate_timeout(id: &str, timeout_seconds: Option<u64>) -> Result<()> {
    if timeout_seconds == Some(0) {
        anyhow::bail!("timeout_seconds for `{id}` must be greater than zero");
    }
    Ok(())
}

fn valid_tool_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous: Vec<_> = (0..=right.chars().count()).collect();
    let mut current = vec![0; previous.len()];
    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            current[right_index + 1] = if left_char == right_char {
                previous[right_index]
            } else {
                1 + previous[right_index]
                    .min(previous[right_index + 1])
                    .min(current[right_index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.chars().count()]
}

pub fn write_initial(path: &Path, project: &Project, force: bool) -> Result<()> {
    write_initial_with_gate(path, project, force, GateProfile::Auto)
}

pub fn write_initial_with_gate(
    path: &Path,
    project: &Project,
    force: bool,
    gate: GateProfile,
) -> Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to replace it",
            path.display()
        );
    }

    let text = initial_text_with_gate(project, gate)?;
    crate::atomic::write(path, text.as_bytes())
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

pub fn initial_text(project: &Project) -> Result<String> {
    initial_text_with_gate(project, GateProfile::Auto)
}

pub fn initial_text_with_gate(project: &Project, gate: GateProfile) -> Result<String> {
    let detected = crate::tools::catalog()
        .into_iter()
        .filter(|tool| tool.detect(project))
        .map(|tool| tool.id.to_owned())
        .collect::<BTreeSet<_>>();
    policy_text_with_tools(project, gate, &detected, None)
}

pub fn merge_preset_text_with_gate(
    project: &Project,
    gate: GateProfile,
    selected_tools: &BTreeSet<String>,
    profile: crate::cli::PresetProfile,
) -> Result<String> {
    let mut config = Config::load_or_default(&project.root)?;
    let canonical_script_detected = detect_repository_check(project, gate).is_some();
    for id in selected_tools {
        let mut tool = config.tools.get(id).cloned().unwrap_or_default();
        tool.enabled = Some(true);
        tool.required = Some(true);
        if !config.tools.contains_key(id) && canonical_script_detected {
            tool.check = Some(false);
        }
        config.tools.insert(id.clone(), tool);
    }
    if config.tasks.is_empty() {
        if let Some(task) = detect_repository_check(project, gate) {
            config.tasks.insert("repository-check".to_owned(), task);
        } else if let Some(task) = detect_typecheck(project) {
            config.tasks.insert("typecheck".to_owned(), task);
        }
    }
    if !config.hooks.contains_key("commit-msg") {
        if let Some(hook) = detect_commitprompt_hook(project) {
            config.hooks.insert("commit-msg".to_owned(), hook);
        }
    }
    let mut text = String::from(
        "# yaml-language-server: $schema=https://quality.santi020k.com/quality.schema.json\n\
         # cspell:ignore actionlint clippy detekt knip ktlint swiftformat swiftlint\n\
         # quality.yml — one code-quality workflow for this repository\n\
         # Preset-managed tools are merged with repository-owned tasks, hooks, and custom adapters.\n",
    );
    text.push_str(&format!(
        "# Generated from the `{profile}` preset; rerun `quality preset update` to refresh it.\n"
    ));
    text.push_str(&serde_yaml::to_string(&config).context("could not serialize configuration")?);
    Ok(text)
}

fn policy_text_with_tools(
    project: &Project,
    gate: GateProfile,
    selected_tools: &BTreeSet<String>,
    preset: Option<crate::cli::PresetProfile>,
) -> Result<String> {
    let repository_task = detect_repository_check(project, gate);
    let canonical_script_detected = repository_task.is_some();
    let mut tools = BTreeMap::new();
    for tool in crate::tools::catalog() {
        if selected_tools.contains(tool.id) {
            tools.insert(
                tool.id.to_owned(),
                ToolConfig {
                    enabled: Some(true),
                    check: canonical_script_detected.then_some(false),
                    required: Some(true),
                    timeout_seconds: None,
                    ..ToolConfig::default()
                },
            );
        }
    }
    let mut tasks = BTreeMap::new();
    if let Some(task) = repository_task {
        tasks.insert("repository-check".to_owned(), task);
    } else if let Some(task) = detect_typecheck(project) {
        tasks.insert("typecheck".to_owned(), task);
    }
    let mut hooks = BTreeMap::new();
    if let Some(hook) = detect_commitprompt_hook(project) {
        hooks.insert("commit-msg".to_owned(), hook);
    }
    let config = Config {
        tools,
        tasks,
        hooks,
        ..Config::default()
    };
    let mut text = String::from(
        "# yaml-language-server: $schema=https://quality.santi020k.com/quality.schema.json\n\
         # cspell:ignore actionlint clippy detekt knip ktlint swiftformat swiftlint\n\
         # quality.yml — one code-quality workflow for this repository\n\
         # Tools are auto-detected; entries below make the selected policy explicit.\n\
         # A canonical repository script replaces analyzer checks when one is detected,\n\
         # while the analyzers remain available to `quality format` and `quality fix`.\n",
    );
    if let Some(profile) = preset {
        text.push_str(&format!(
            "# Generated from the `{profile}` preset; rerun `quality preset apply {profile}` to refresh it.\n"
        ));
    }
    text.push_str(&serde_yaml::to_string(&config).context("could not serialize configuration")?);
    Ok(text)
}

fn detect_repository_check(project: &Project, gate: GateProfile) -> Option<TaskConfig> {
    let path = project.root.join("package.json");
    let text = fs::read_to_string(path).ok()?;
    let manifest = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let scripts = manifest.get("scripts")?.as_object()?;
    let candidates: &[&str] = match gate {
        GateProfile::Auto => &[
            "verify:quality",
            "verify",
            "validate",
            "check",
            "pre-push",
            "prepush",
        ],
        GateProfile::Fast => &[
            "verify:fast",
            "verify:quality",
            "check",
            "typecheck",
            "type-check",
        ],
        GateProfile::Full => &[
            "verify:full",
            "validate",
            "verify",
            "pre-push",
            "prepush",
            "check",
        ],
    };
    let script = candidates.iter().copied().find_map(|name| {
        scripts.get(name)?.as_str()?;
        (!script_invokes_quality(scripts, name, &mut BTreeSet::new())).then_some(name)
    })?;

    Some(TaskConfig {
        name: Some(format!("Repository check ({script})")),
        command: PathBuf::from(package_manager(project, &manifest)),
        args: vec!["run".to_owned(), script.to_owned()],
        working_directory: None,
        required: true,
        extensions: Vec::new(),
        config_files: Vec::new(),
        parser: DiagnosticParser::Generic,
        timeout_seconds: None,
    })
}

fn detect_typecheck(project: &Project) -> Option<TaskConfig> {
    let path = project.root.join("package.json");
    let text = fs::read_to_string(path).ok()?;
    let manifest = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let scripts = manifest.get("scripts")?.as_object()?;
    let script = ["typecheck", "type-check"].into_iter().find(|name| {
        scripts
            .get(*name)
            .and_then(|value| value.as_str())
            .is_some()
    })?;

    Some(TaskConfig {
        name: Some("TypeScript".to_owned()),
        command: PathBuf::from(package_manager(project, &manifest)),
        args: vec!["run".to_owned(), script.to_owned()],
        working_directory: None,
        required: true,
        extensions: vec![
            "astro".to_owned(),
            "js".to_owned(),
            "jsx".to_owned(),
            "ts".to_owned(),
            "tsx".to_owned(),
        ],
        config_files: vec![
            "package.json".to_owned(),
            "tsconfig.json".to_owned(),
            "pnpm-lock.yaml".to_owned(),
            "yarn.lock".to_owned(),
            "package-lock.json".to_owned(),
        ],
        parser: DiagnosticParser::Generic,
        timeout_seconds: None,
    })
}

fn detect_commitprompt_hook(project: &Project) -> Option<HookConfig> {
    let path = project.root.join("package.json");
    let text = fs::read_to_string(path).ok()?;
    let manifest = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let installed = [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ]
    .into_iter()
    .filter_map(|field| manifest.get(field).and_then(serde_json::Value::as_object))
    .any(|dependencies| dependencies.contains_key("@santi020k/commitprompt"));
    if !installed {
        return None;
    }

    let manager = package_manager(project, &manifest);
    let mut args = match manager {
        "npm" => vec!["exec", "--", "commitprompt"],
        "bun" => vec!["x", "commitprompt"],
        _ => vec!["exec", "commitprompt"],
    }
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    args.extend(["validate", "--input"].map(str::to_owned));

    Some(HookConfig {
        steps: vec![HookStepConfig {
            name: Some("Validate commit message".to_owned()),
            command: PathBuf::from(manager),
            args,
            working_directory: None,
            pass_hook_args: true,
        }],
    })
}

fn package_manager<'a>(project: &Project, manifest: &'a serde_json::Value) -> &'a str {
    manifest
        .get("packageManager")
        .and_then(|value| value.as_str())
        .and_then(|value| value.split('@').next())
        .filter(|value| matches!(*value, "pnpm" | "yarn" | "npm" | "bun"))
        .unwrap_or_else(|| {
            if project.root.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else if project.root.join("yarn.lock").exists() {
                "yarn"
            } else if project.root.join("bun.lock").exists()
                || project.root.join("bun.lockb").exists()
            {
                "bun"
            } else {
                "npm"
            }
        })
}

fn invokes_quality_check(command: &str) -> bool {
    command.contains("quality check")
        || (command.contains("quality-cli") && command.contains(" check"))
}

fn script_invokes_quality(
    scripts: &serde_json::Map<String, serde_json::Value>,
    script: &str,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(script.to_owned()) {
        return false;
    }
    let invokes_quality = scripts
        .get(script)
        .and_then(|value| value.as_str())
        .is_some_and(|command| {
            invokes_quality_check(command)
                || scripts.keys().any(|dependency| {
                    command_invokes_script(command, dependency)
                        && script_invokes_quality(scripts, dependency, visiting)
                })
        });
    visiting.remove(script);
    invokes_quality
}

fn command_invokes_script(command: &str, script: &str) -> bool {
    let tokens: Vec<_> = command
        .split(|character: char| {
            character.is_ascii_whitespace()
                || matches!(character, ';' | '&' | '|' | '(' | ')' | '"' | '\'')
        })
        .filter(|token| !token.is_empty())
        .collect();
    tokens
        .windows(2)
        .any(|tokens| tokens[0] == "run" && tokens[1] == script)
        || tokens
            .windows(2)
            .any(|tokens| matches!(tokens[0], "yarn" | "pnpm" | "bun") && tokens[1] == script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_handles_insertions_and_transpositions() {
        assert_eq!(edit_distance("swiftlint", "swiftlint"), 0);
        assert_eq!(edit_distance("swfitlint", "swiftlint"), 2);
        assert_eq!(edit_distance("ktlin", "ktlint"), 1);
    }

    #[test]
    fn published_schema_lists_every_builtin_tool() {
        let schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/site/public/quality.schema.json"
        ))
        .unwrap();
        let schema_tools: BTreeSet<_> = schema["properties"]["tools"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let catalog_tools: BTreeSet<_> = crate::tools::catalog()
            .into_iter()
            .map(|tool| tool.id)
            .collect();

        assert_eq!(schema_tools, catalog_tools);
    }
}
