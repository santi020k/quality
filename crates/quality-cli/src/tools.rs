use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::changes::ChangeSet;
use crate::config::{DiagnosticParser, ExternalFileMode, ExternalToolConfig, ToolConfig};
use crate::project::Project;
use crate::runner::Operation;

#[derive(Clone, Debug)]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub executable: &'static str,
    pub install_hint: &'static str,
    detector: fn(&Project) -> bool,
    check_args: &'static [&'static str],
    format_args: Option<&'static [&'static str]>,
    fix_args: Option<&'static [&'static str]>,
}

#[derive(Clone, Debug)]
pub struct Invocation {
    pub id: String,
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub parser: DiagnosticParser,
    pub required: bool,
    pub install_hint: String,
}

impl Tool {
    pub fn detect(&self, project: &Project) -> bool {
        (self.detector)(project)
    }

    pub fn invocation(
        &self,
        project: &Project,
        config: &ToolConfig,
        operation: Operation,
        changes: Option<&ChangeSet>,
    ) -> Option<Invocation> {
        let enabled = config.enabled.unwrap_or_else(|| self.detect(project));
        if !enabled {
            return None;
        }

        let defaults = match operation {
            Operation::Check => Some(self.check_args),
            Operation::CheckFormat => self.format_args.map(|_| self.check_args),
            Operation::Format => self.format_args,
            Operation::Fix => self.fix_args.or(self.format_args),
        }?;
        let configured = match operation {
            Operation::Check | Operation::CheckFormat => &config.check_args,
            Operation::Format => &config.format_args,
            Operation::Fix => &config.fix_args,
        };
        // A command override is assumed to be CLI-compatible (for example, a pinned
        // binary). Custom arguments opt out because appending paths to an arbitrary
        // Gradle or wrapper task may change its meaning.
        let customized = configured.is_some();
        let mut args = configured
            .clone()
            .unwrap_or_else(|| defaults.iter().map(|value| (*value).to_owned()).collect());
        let mut env = BTreeMap::new();

        if let Some(changes) = changes {
            let relevant: Vec<_> = changes
                .files
                .iter()
                .filter(|path| self.accepts_changed_path(path))
                .cloned()
                .collect();
            let configuration_changed = changes
                .files
                .iter()
                .any(|path| self.configuration_path(path));
            if relevant.is_empty() && !configuration_changed {
                return None;
            }
            if !customized && !configuration_changed {
                self.scope_args(project, &relevant, &mut args, &mut env);
            }
        }

        let command = config.command.clone().unwrap_or_else(|| {
            if self.id == "android-lint" {
                let wrapper = project.root.join("gradlew");
                if wrapper.exists() {
                    return wrapper;
                }
            }
            if matches!(self.id, "eslint" | "prettier") {
                let local = project.root.join("node_modules/.bin").join(self.executable);
                if local.exists() {
                    return local;
                }
            }
            PathBuf::from(self.executable)
        });

        Some(Invocation {
            id: self.id.to_owned(),
            name: self.name.to_owned(),
            command,
            args,
            env,
            parser: match self.id {
                "eslint" => DiagnosticParser::EslintJson,
                "swiftlint" => DiagnosticParser::SwiftlintJson,
                "ktlint" => DiagnosticParser::KtlintJson,
                _ => DiagnosticParser::Generic,
            },
            required: config.required.unwrap_or(true),
            install_hint: self.install_hint.to_owned(),
        })
    }

    fn accepts_changed_path(&self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        match self.id {
            "swiftlint" | "swiftformat" => extension == "swift",
            "android-lint" => matches!(
                extension,
                "kt" | "kts" | "java" | "xml" | "gradle" | "properties" | "toml"
            ),
            "detekt" | "ktlint" => matches!(extension, "kt" | "kts"),
            "eslint" => matches!(extension, "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs"),
            "prettier" => matches!(
                extension,
                "js" | "jsx"
                    | "ts"
                    | "tsx"
                    | "mjs"
                    | "cjs"
                    | "json"
                    | "json5"
                    | "css"
                    | "scss"
                    | "less"
                    | "html"
                    | "md"
                    | "mdx"
                    | "yaml"
                    | "yml"
                    | "graphql"
            ),
            _ => false,
        }
    }

    fn configuration_path(&self, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            return false;
        };
        if name == "quality.yml" {
            return true;
        }
        match self.id {
            "swiftlint" => name == ".swiftlint.yml",
            "swiftformat" => name == ".swiftformat",
            "android-lint" => matches!(
                name,
                "lint.xml"
                    | "build.gradle"
                    | "build.gradle.kts"
                    | "settings.gradle"
                    | "settings.gradle.kts"
                    | "gradle.properties"
                    | "libs.versions.toml"
            ),
            "detekt" => matches!(
                name,
                "detekt.yml" | "detekt.yaml" | "build.gradle" | "build.gradle.kts"
            ),
            "ktlint" => name == ".editorconfig",
            "eslint" => {
                name == "package.json"
                    || name.starts_with("eslint.config.")
                    || name.starts_with(".eslintrc")
            }
            "prettier" => {
                name == "package.json"
                    || name.starts_with(".prettierrc")
                    || name.starts_with("prettier.config.")
            }
            _ => false,
        }
    }

    fn scope_args(
        &self,
        project: &Project,
        files: &[PathBuf],
        args: &mut Vec<String>,
        env: &mut BTreeMap<String, String>,
    ) {
        match self.id {
            "swiftlint" => {
                args.push("--use-script-input-files".to_owned());
                env.insert(
                    "SCRIPT_INPUT_FILE_COUNT".to_owned(),
                    files.len().to_string(),
                );
                for (index, path) in files.iter().enumerate() {
                    env.insert(
                        format!("SCRIPT_INPUT_FILE_{index}"),
                        project.root.join(path).display().to_string(),
                    );
                }
            }
            "swiftformat" | "eslint" | "prettier" => {
                args.retain(|arg| arg != ".");
                args.extend(files.iter().map(|path| path.display().to_string()));
            }
            "ktlint" => {
                args.extend(files.iter().map(|path| path.display().to_string()));
            }
            "detekt" => {
                if let Ok(paths) = std::env::join_paths(files) {
                    args.push("--input".to_owned());
                    args.push(paths.to_string_lossy().to_string());
                }
            }
            // Android Lint analyzes a Gradle project as a unit.
            "android-lint" => {}
            _ => {}
        }
    }
}

pub fn external_detects(project: &Project, config: &ExternalToolConfig) -> bool {
    config.extensions.is_empty()
        || config
            .extensions
            .iter()
            .any(|extension| project.has_extension(extension))
}

pub fn external_invocation(
    id: &str,
    config: &ExternalToolConfig,
    project: &Project,
    operation: Operation,
    changes: Option<&ChangeSet>,
) -> Option<Invocation> {
    if !config.enabled || !external_detects(project, config) {
        return None;
    }
    let mut args = match operation {
        Operation::Check => config.check_args.clone(),
        Operation::CheckFormat => config.format_check_args.clone()?,
        Operation::Format => config.format_args.clone()?,
        Operation::Fix => config
            .fix_args
            .clone()
            .or_else(|| config.format_args.clone())?,
    };

    if let Some(changes) = changes {
        let relevant: Vec<_> = changes
            .files
            .iter()
            .filter(|path| external_accepts_path(path, config))
            .cloned()
            .collect();
        let configuration_changed = changes
            .files
            .iter()
            .any(|path| external_configuration_path(path, config));
        if relevant.is_empty() && !configuration_changed {
            return None;
        }
        if !configuration_changed && matches!(config.file_mode, ExternalFileMode::Append) {
            args.extend(relevant.iter().map(|path| path.display().to_string()));
        }
    }

    Some(Invocation {
        id: id.to_owned(),
        name: config.name.clone().unwrap_or_else(|| id.to_owned()),
        command: config.command.clone(),
        args,
        env: BTreeMap::new(),
        parser: config.parser,
        required: config.required,
        install_hint: config.install_hint.clone().unwrap_or_else(|| {
            format!(
                "Install or configure the `{id}` command declared under `custom` in quality.yml."
            )
        }),
    })
}

fn external_accepts_path(path: &Path, config: &ExternalToolConfig) -> bool {
    if config.extensions.is_empty() {
        return true;
    }
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| config.extensions.iter().any(|value| value == extension))
}

fn external_configuration_path(path: &Path, config: &ExternalToolConfig) -> bool {
    let rendered = path.to_string_lossy();
    let name = path.file_name().and_then(|value| value.to_str());
    config.config_files.iter().any(|configured| {
        configured == rendered.as_ref() || name.is_some_and(|name| name == configured)
    }) || name == Some("quality.yml")
}

pub fn catalog() -> Vec<Tool> {
    vec![
        Tool {
            id: "swiftlint",
            name: "SwiftLint",
            executable: "swiftlint",
            install_hint: "Install SwiftLint with Homebrew (`brew install swiftlint`) or a SwiftPM plugin.",
            detector: detects_swift,
            check_args: &["lint", "--quiet", "--reporter", "json"],
            format_args: None,
            fix_args: Some(&["--fix"]),
        },
        Tool {
            id: "swiftformat",
            name: "SwiftFormat",
            executable: "swiftformat",
            install_hint: "Install SwiftFormat with Homebrew (`brew install swiftformat`) or a SwiftPM plugin.",
            detector: detects_swift,
            check_args: &["--lint", "."],
            format_args: Some(&["."]),
            fix_args: Some(&["."]),
        },
        Tool {
            id: "android-lint",
            name: "Android Lint",
            executable: "gradle",
            install_hint: "Add the Gradle wrapper to the Android project; quality will run `./gradlew lint`.",
            detector: detects_android,
            check_args: &["lint"],
            format_args: None,
            fix_args: None,
        },
        Tool {
            id: "detekt",
            name: "detekt",
            executable: "detekt",
            install_hint: "Configure the detekt Gradle plugin or install the detekt CLI.",
            detector: detects_kotlin,
            check_args: &[],
            format_args: None,
            fix_args: None,
        },
        Tool {
            id: "ktlint",
            name: "ktlint",
            executable: "ktlint",
            install_hint: "Install ktlint or configure its Gradle plugin.",
            detector: detects_kotlin,
            check_args: &["--relative", "--log-level=none", "--reporter=json"],
            format_args: Some(&["--format", "--relative", "--log-level=none"]),
            fix_args: Some(&["--format", "--relative", "--log-level=none"]),
        },
        Tool {
            id: "eslint",
            name: "ESLint",
            executable: "eslint",
            install_hint: "Install ESLint in the repository (`npm install --save-dev eslint`).",
            detector: detects_javascript,
            check_args: &[".", "--format", "json"],
            format_args: None,
            fix_args: Some(&[".", "--fix"]),
        },
        Tool {
            id: "prettier",
            name: "Prettier",
            executable: "prettier",
            install_hint: "Install Prettier in the repository (`npm install --save-dev prettier`).",
            detector: detects_javascript,
            check_args: &[".", "--check"],
            format_args: Some(&[".", "--write"]),
            fix_args: Some(&[".", "--write"]),
        },
    ]
}

fn detects_swift(project: &Project) -> bool {
    project.has_extension("swift")
        || project.has_file("Package.swift")
        || project.path_contains(".xcodeproj/")
}

fn detects_kotlin(project: &Project) -> bool {
    project.has_extension("kt") || project.has_extension("kts")
}

fn detects_android(project: &Project) -> bool {
    project.has_file("AndroidManifest.xml")
}

fn detects_javascript(project: &Project) -> bool {
    project.has_file("package.json")
        || ["js", "jsx", "ts", "tsx"]
            .iter()
            .any(|extension| project.has_extension(extension))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::ChangeSet;

    #[test]
    fn detects_a_mixed_mobile_repository() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("App.swift"), "").unwrap();
        std::fs::write(temp.path().join("MainActivity.kt"), "").unwrap();
        std::fs::write(temp.path().join("build.gradle.kts"), "").unwrap();
        std::fs::write(temp.path().join("AndroidManifest.xml"), "").unwrap();
        let project = Project::discover(temp.path()).unwrap();
        let detected: Vec<_> = catalog()
            .into_iter()
            .filter(|tool| tool.detect(&project))
            .map(|tool| tool.id)
            .collect();
        assert_eq!(
            detected,
            vec![
                "swiftlint",
                "swiftformat",
                "android-lint",
                "detekt",
                "ktlint"
            ]
        );
    }

    #[test]
    fn configuration_changes_keep_swiftlint_at_full_project_scope() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("App.swift"), "").unwrap();
        let project = Project::discover(temp.path()).unwrap();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.id == "swiftlint")
            .unwrap();
        let changes = ChangeSet {
            base: "HEAD".to_owned(),
            files: vec![PathBuf::from(".swiftlint.yml")],
        };

        let invocation = tool
            .invocation(
                &project,
                &ToolConfig::default(),
                Operation::Check,
                Some(&changes),
            )
            .unwrap();
        assert!(
            !invocation
                .args
                .contains(&"--use-script-input-files".to_owned())
        );
        assert!(invocation.env.is_empty());
    }

    #[test]
    fn android_lint_stays_project_wide_for_changed_kotlin() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("AndroidManifest.xml"), "").unwrap();
        std::fs::write(temp.path().join("MainActivity.kt"), "").unwrap();
        let project = Project::discover(temp.path()).unwrap();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.id == "android-lint")
            .unwrap();
        let changes = ChangeSet {
            base: "HEAD".to_owned(),
            files: vec![PathBuf::from("MainActivity.kt")],
        };

        let invocation = tool
            .invocation(
                &project,
                &ToolConfig::default(),
                Operation::Check,
                Some(&changes),
            )
            .unwrap();
        assert_eq!(invocation.args, vec!["lint"]);
    }
}
