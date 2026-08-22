use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::changes::ChangeSet;
use crate::config::{
    DiagnosticParser, ExternalFileMode, ExternalToolConfig, TaskConfig, ToolConfig,
};
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
    pub working_directory: PathBuf,
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

    #[cfg(test)]
    pub fn invocation(
        &self,
        project: &Project,
        config: &ToolConfig,
        operation: Operation,
        changes: Option<&ChangeSet>,
    ) -> Option<Invocation> {
        self.invocations(project, config, operation, changes)
            .into_iter()
            .next()
    }

    pub fn invocations(
        &self,
        project: &Project,
        config: &ToolConfig,
        operation: Operation,
        changes: Option<&ChangeSet>,
    ) -> Vec<Invocation> {
        let enabled = config.enabled.unwrap_or_else(|| self.detect(project));
        if !enabled {
            return Vec::new();
        }

        self.working_directories(project, config)
            .into_iter()
            .filter_map(|working_directory| {
                self.invocation_at(project, config, operation, changes, working_directory)
            })
            .collect()
    }

    fn invocation_at(
        &self,
        project: &Project,
        config: &ToolConfig,
        operation: Operation,
        changes: Option<&ChangeSet>,
        working_directory: PathBuf,
    ) -> Option<Invocation> {
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
            let workspace = working_directory
                .strip_prefix(&project.root)
                .unwrap_or(Path::new(""));
            let relevant: Vec<_> = changes
                .files
                .iter()
                .filter_map(|path| path.strip_prefix(workspace).ok())
                .filter(|path| self.accepts_changed_path(path))
                .map(Path::to_path_buf)
                .collect();
            let configuration_changed = changes
                .files
                .iter()
                .any(|path| self.configuration_path(path));
            if relevant.is_empty() && !configuration_changed {
                return None;
            }
            if !customized && !configuration_changed {
                self.scope_args(&working_directory, &relevant, &mut args, &mut env);
            }
        }

        let command = config.command.clone().unwrap_or_else(|| {
            if self.id == "android-lint" {
                let wrapper = working_directory.join("gradlew");
                if wrapper.exists() {
                    return wrapper;
                }
            }
            if matches!(self.id, "eslint" | "prettier" | "astro-check") {
                let local = working_directory
                    .join("node_modules/.bin")
                    .join(self.executable);
                if local.exists() {
                    return local;
                }
                let root_local = project.root.join("node_modules/.bin").join(self.executable);
                if root_local.exists() {
                    return root_local;
                }
            }
            PathBuf::from(self.executable)
        });

        let relative_workspace = working_directory
            .strip_prefix(&project.root)
            .unwrap_or(Path::new(""));
        let nested = !relative_workspace.as_os_str().is_empty();

        Some(Invocation {
            id: if nested {
                format!("{}@{}", self.id, relative_workspace.display())
            } else {
                self.id.to_owned()
            },
            name: if nested {
                format!("{} ({})", self.name, relative_workspace.display())
            } else {
                self.name.to_owned()
            },
            command,
            working_directory,
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

    fn working_directories(&self, project: &Project, config: &ToolConfig) -> Vec<PathBuf> {
        if let Some(directory) = &config.working_directory {
            return vec![workspace_path(&project.root, directory)];
        }
        let relative = match self.id {
            "android-lint" => android_workspaces(project),
            "detekt" | "ktlint" => kotlin_workspaces(project),
            "swiftlint" | "swiftformat" => swift_workspaces(project),
            "cargo-fmt" | "cargo-clippy" => cargo_workspaces(project),
            "astro-check" => astro_workspaces(project),
            _ => vec![PathBuf::new()],
        };
        relative
            .into_iter()
            .map(|directory| workspace_path(&project.root, &directory))
            .collect()
    }

    fn accepts_changed_path(&self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        match self.id {
            "swiftlint" | "swiftformat" => extension == "swift",
            "cargo-fmt" | "cargo-clippy" => matches!(extension, "rs" | "toml"),
            "astro-check" => matches!(
                extension,
                "astro" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
            ),
            "android-lint" => matches!(
                extension,
                "kt" | "kts" | "java" | "xml" | "gradle" | "properties" | "toml"
            ),
            "detekt" | "ktlint" => matches!(extension, "kt" | "kts"),
            "eslint" => matches!(
                extension,
                "js" | "jsx"
                    | "ts"
                    | "tsx"
                    | "mjs"
                    | "cjs"
                    | "mts"
                    | "cts"
                    | "astro"
                    | "vue"
                    | "svelte"
            ),
            "prettier" => matches!(
                extension,
                "js" | "jsx"
                    | "ts"
                    | "tsx"
                    | "mjs"
                    | "cjs"
                    | "mts"
                    | "cts"
                    | "astro"
                    | "vue"
                    | "svelte"
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
            "cargo-fmt" | "cargo-clippy" => matches!(
                name,
                "Cargo.toml" | "Cargo.lock" | "rustfmt.toml" | ".rustfmt.toml"
            ),
            "astro-check" => {
                name == "package.json"
                    || name == "tsconfig.json"
                    || name.starts_with("astro.config.")
                    || javascript_workspace_configuration(name)
            }
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
                    || javascript_workspace_configuration(name)
            }
            "prettier" => {
                name == "package.json"
                    || name.starts_with(".prettierrc")
                    || name.starts_with("prettier.config.")
                    || javascript_workspace_configuration(name)
            }
            _ => false,
        }
    }

    fn scope_args(
        &self,
        working_directory: &Path,
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
                        working_directory.join(path).display().to_string(),
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
            // These adapters analyze their workspace as a unit.
            "android-lint" | "cargo-fmt" | "cargo-clippy" | "astro-check" => {}
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

pub fn task_invocation(
    id: &str,
    config: &TaskConfig,
    project: &Project,
    operation: Operation,
    changes: Option<&ChangeSet>,
) -> Option<Invocation> {
    if !matches!(operation, Operation::Check) {
        return None;
    }
    let working_directory = workspace_path(
        &project.root,
        config
            .working_directory
            .as_deref()
            .unwrap_or_else(|| Path::new("")),
    );
    let workspace = working_directory
        .strip_prefix(&project.root)
        .unwrap_or(Path::new(""));
    if let Some(changes) = changes
        && (!config.extensions.is_empty() || !config.config_files.is_empty())
    {
        let relevant = changes.files.iter().any(|path| {
            configured_path(path, &config.config_files)
                || path.strip_prefix(workspace).is_ok_and(|path| {
                    (!config.extensions.is_empty()
                        && external_accepts_extensions(path, &config.extensions))
                        || configured_path(path, &config.config_files)
                })
        });
        let global_configuration_changed = changes
            .files
            .iter()
            .any(|path| path == Path::new("quality.yml"));
        if !relevant && !global_configuration_changed {
            return None;
        }
    }

    Some(Invocation {
        id: id.to_owned(),
        name: config.name.clone().unwrap_or_else(|| id.to_owned()),
        command: config.command.clone(),
        working_directory,
        args: config.args.clone(),
        env: BTreeMap::new(),
        parser: config.parser,
        required: config.required,
        install_hint: format!(
            "Install or configure the `{id}` command declared under `tasks` in quality.yml."
        ),
    })
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

    let working_directory = workspace_path(
        &project.root,
        config
            .working_directory
            .as_deref()
            .unwrap_or_else(|| Path::new("")),
    );
    let workspace = working_directory
        .strip_prefix(&project.root)
        .unwrap_or(Path::new(""));

    if let Some(changes) = changes {
        let relevant: Vec<_> = changes
            .files
            .iter()
            .filter_map(|path| path.strip_prefix(workspace).ok())
            .filter(|path| external_accepts_path(path, config))
            .map(Path::to_path_buf)
            .collect();
        let configuration_changed = changes
            .files
            .iter()
            .any(|path| path == Path::new("quality.yml"))
            || changes
                .files
                .iter()
                .any(|path| external_configuration_path(path, config))
            || changes
                .files
                .iter()
                .filter_map(|path| path.strip_prefix(workspace).ok())
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
        working_directory,
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
    external_accepts_extensions(path, &config.extensions)
}

fn external_accepts_extensions(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extensions.iter().any(|value| value == extension))
}

fn external_configuration_path(path: &Path, config: &ExternalToolConfig) -> bool {
    configured_path(path, &config.config_files)
        || path.file_name().and_then(|value| value.to_str()) == Some("quality.yml")
}

fn configured_path(path: &Path, config_files: &[String]) -> bool {
    let rendered = path.to_string_lossy();
    let name = path.file_name().and_then(|value| value.to_str());
    config_files.iter().any(|configured| {
        configured == rendered.as_ref() || name.is_some_and(|name| name == configured)
    })
}

fn workspace_path(root: &Path, directory: &Path) -> PathBuf {
    if directory.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(directory)
    }
}

pub fn catalog() -> Vec<Tool> {
    vec![
        Tool {
            id: "cargo-fmt",
            name: "Cargo fmt",
            executable: "cargo",
            install_hint: "Install the Rust toolchain with the rustfmt component.",
            detector: detects_rust,
            check_args: &["fmt", "--all", "--", "--check"],
            format_args: Some(&["fmt", "--all"]),
            fix_args: Some(&["fmt", "--all"]),
        },
        Tool {
            id: "cargo-clippy",
            name: "Clippy",
            executable: "cargo",
            install_hint: "Install the Rust toolchain with the clippy component.",
            detector: detects_rust,
            check_args: &[
                "clippy",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            format_args: None,
            fix_args: None,
        },
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
            detector: detects_detekt,
            check_args: &[],
            format_args: None,
            fix_args: None,
        },
        Tool {
            id: "ktlint",
            name: "ktlint",
            executable: "ktlint",
            install_hint: "Install ktlint or configure its Gradle plugin.",
            detector: detects_ktlint,
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
            id: "astro-check",
            name: "Astro Check",
            executable: "astro",
            install_hint: "Install Astro and @astrojs/check in the repository.",
            detector: detects_astro,
            check_args: &["check"],
            format_args: None,
            fix_args: None,
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

fn detects_rust(project: &Project) -> bool {
    project.has_file("Cargo.toml") || project.has_extension("rs")
}

fn detects_astro(project: &Project) -> bool {
    [
        "astro.config.js",
        "astro.config.mjs",
        "astro.config.cjs",
        "astro.config.ts",
        "astro.config.mts",
    ]
    .iter()
    .any(|name| project.has_file(name))
}

fn detects_kotlin(project: &Project) -> bool {
    project.has_extension("kt") || project.has_extension("kts")
}

fn detects_detekt(project: &Project) -> bool {
    detects_kotlin(project) && (project.has_file("detekt.yml") || project.has_file("detekt.yaml"))
}

fn detects_ktlint(project: &Project) -> bool {
    detects_kotlin(project) && project.has_file(".editorconfig")
}

fn detects_android(project: &Project) -> bool {
    project.has_file("AndroidManifest.xml")
}

fn detects_javascript(project: &Project) -> bool {
    project.has_file("package.json")
        || [
            "js", "jsx", "ts", "tsx", "mts", "cts", "astro", "vue", "svelte",
        ]
        .iter()
        .any(|extension| project.has_extension(extension))
}

fn javascript_workspace_configuration(name: &str) -> bool {
    matches!(
        name,
        "pnpm-lock.yaml"
            | "pnpm-workspace.yaml"
            | "package-lock.json"
            | "yarn.lock"
            | "bun.lock"
            | "bun.lockb"
            | "tsconfig.json"
            | "tsconfig.base.json"
    )
}

fn android_workspaces(project: &Project) -> Vec<PathBuf> {
    let mut roots: std::collections::BTreeSet<_> = project
        .paths_named("gradlew")
        .filter_map(Path::parent)
        .map(Path::to_path_buf)
        .collect();
    if roots.is_empty() {
        roots.insert(PathBuf::new());
    }
    roots.into_iter().collect()
}

fn kotlin_workspaces(project: &Project) -> Vec<PathBuf> {
    let roots = project
        .paths_named("gradlew")
        .filter_map(Path::parent)
        .map(Path::to_path_buf);
    outermost_workspaces(roots)
}

fn swift_workspaces(project: &Project) -> Vec<PathBuf> {
    let mut roots: std::collections::BTreeSet<_> = project
        .paths_named("Package.swift")
        .filter_map(Path::parent)
        .map(Path::to_path_buf)
        .collect();
    roots.extend(
        project
            .paths_named("project.pbxproj")
            .filter_map(Path::parent)
            .filter_map(Path::parent)
            .map(Path::to_path_buf),
    );
    if roots.is_empty() {
        roots.insert(PathBuf::new());
    }
    roots.into_iter().collect()
}

fn cargo_workspaces(project: &Project) -> Vec<PathBuf> {
    outermost_workspaces(
        project
            .paths_named("Cargo.toml")
            .filter_map(Path::parent)
            .map(Path::to_path_buf),
    )
}

fn astro_workspaces(project: &Project) -> Vec<PathBuf> {
    let roots = [
        "astro.config.js",
        "astro.config.mjs",
        "astro.config.cjs",
        "astro.config.ts",
        "astro.config.mts",
    ]
    .into_iter()
    .flat_map(|name| project.paths_named(name))
    .filter_map(Path::parent)
    .map(Path::to_path_buf);
    unique_workspaces(roots)
}

fn unique_workspaces(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut selected: std::collections::BTreeSet<_> = roots.into_iter().collect();
    if selected.is_empty() {
        selected.insert(PathBuf::new());
    }
    selected.into_iter().collect()
}

fn outermost_workspaces(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut candidates: Vec<_> = roots.into_iter().collect();
    candidates.sort_by_key(|path| path.components().count());
    let mut selected: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !selected.iter().any(|root| candidate.starts_with(root)) {
            selected.push(candidate);
        }
    }
    if selected.is_empty() {
        selected.push(PathBuf::new());
    }
    selected
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
        std::fs::write(temp.path().join("detekt.yml"), "").unwrap();
        std::fs::write(temp.path().join(".editorconfig"), "").unwrap();
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
