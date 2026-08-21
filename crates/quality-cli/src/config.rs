use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::OutputFormat;
use crate::project::Project;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub output: OutputFormat,
    pub baseline: PathBuf,
    pub tools: BTreeMap<String, ToolConfig>,
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
            custom_tools: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_args: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticParser {
    #[default]
    Generic,
    EslintJson,
    SwiftlintJson,
    KtlintJson,
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
        for (id, tool) in &self.custom_tools {
            if supported.contains(&id.as_str()) {
                anyhow::bail!(
                    "custom tool `{id}` conflicts with a built-in tool; configure the built-in under `tools` instead"
                );
            }
            if !valid_tool_id(id) {
                anyhow::bail!(
                    "invalid custom tool ID `{id}`; use lowercase letters, numbers, hyphens, or underscores"
                );
            }
            if tool.command.as_os_str().is_empty() {
                anyhow::bail!("custom tool `{id}` must define a non-empty command");
            }
            if let Some(extension) = tool.extensions.iter().find(|value| {
                value.is_empty()
                    || value.starts_with('.')
                    || !value
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
            }) {
                anyhow::bail!(
                    "invalid extension `{extension}` for custom tool `{id}`; write extensions without a leading dot"
                );
            }
        }
        Ok(())
    }
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
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to replace it",
            path.display()
        );
    }

    let mut tools = BTreeMap::new();
    for tool in crate::tools::catalog() {
        if tool.detect(project) {
            tools.insert(
                tool.id.to_owned(),
                ToolConfig {
                    enabled: Some(true),
                    required: Some(true),
                    ..ToolConfig::default()
                },
            );
        }
    }
    let config = Config {
        tools,
        ..Config::default()
    };
    let mut text = String::from(
        "# quality.yml — one code-quality workflow for this repository\n\
         # Tools are auto-detected; entries below make the selected policy explicit.\n",
    );
    text.push_str(&serde_yaml::to_string(&config).context("could not serialize configuration")?);
    fs::write(path, text).with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
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
}
