use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::{GateProfile, PresetEcosystem, PresetProfile};
use crate::project::Project;

const ESLINT_CONFIG_VERSION: &str = "3.5.1";
const ESLINT_VERSION: &str = "10.9.0";
const PRETTIER_VERSION: &str = "3.9.6";
const CSPELL_VERSION: &str = "10.1.0";
const CODESPELL_VERSION: &str = "2.4.3";
const KNIP_VERSION: &str = "6.32.2";
const TYPOS_VERSION: &str = "1.49.0";
const TYPESCRIPT_VERSION: &str = "6.0.3";
const ESLINT_ASTRO_VERSION: &str = "3.1.2";
const ESLINT_EXTENSIONS_VERSION: &str = "3.1.1";
const PRESET_METADATA: &str = ".quality-preset.json";
const PRESET_SCHEMA: &str = "https://quality.santi020k.com/quality-preset.schema.json";
const PRESET_SCHEMA_VERSION: u32 = 1;
pub const PRESET_CATALOG_VERSION: u32 = 2;
const MANAGED_START: &str = "# quality-preset:start";
const MANAGED_END: &str = "# quality-preset:end";

const FRAMEWORK_PRESETS: &[FrameworkPreset] = &[
    FrameworkPreset {
        name: "angular",
        package: "@santi020k/eslint-config-angular",
        version: "3.1.0",
        signals: &["@angular/core"],
    },
    FrameworkPreset {
        name: "astro",
        package: "@santi020k/eslint-config-astro",
        version: ESLINT_ASTRO_VERSION,
        signals: &["astro"],
    },
    FrameworkPreset {
        name: "expo",
        package: "@santi020k/eslint-config-expo",
        version: "3.1.0",
        signals: &["expo", "react-native"],
    },
    FrameworkPreset {
        name: "hono",
        package: "@santi020k/eslint-config-hono",
        version: "3.1.0",
        signals: &["hono"],
    },
    FrameworkPreset {
        name: "lit",
        package: "@santi020k/eslint-config-lit",
        version: "3.1.0",
        signals: &["lit", "lit-element"],
    },
    FrameworkPreset {
        name: "nest",
        package: "@santi020k/eslint-config-nest",
        version: "3.1.0",
        signals: &["@nestjs/core"],
    },
    FrameworkPreset {
        name: "next",
        package: "@santi020k/eslint-config-next",
        version: "3.1.0",
        signals: &["next"],
    },
    FrameworkPreset {
        name: "nuxt",
        package: "@santi020k/eslint-config-nuxt",
        version: "3.1.0",
        signals: &["nuxt"],
    },
    FrameworkPreset {
        name: "preact",
        package: "@santi020k/eslint-config-preact",
        version: "3.1.0",
        signals: &["preact"],
    },
    FrameworkPreset {
        name: "qwik",
        package: "@santi020k/eslint-config-qwik",
        version: "3.1.0",
        signals: &["@builder.io/qwik"],
    },
    FrameworkPreset {
        name: "react",
        package: "@santi020k/eslint-config-react",
        version: "3.1.0",
        signals: &["react"],
    },
    FrameworkPreset {
        name: "react-router",
        package: "@santi020k/eslint-config-react-router",
        version: "3.1.0",
        signals: &["@react-router/dev", "@remix-run/react"],
    },
    FrameworkPreset {
        name: "slidev",
        package: "@santi020k/eslint-config-slidev",
        version: "3.1.0",
        signals: &["@slidev/cli"],
    },
    FrameworkPreset {
        name: "solid",
        package: "@santi020k/eslint-config-solid",
        version: "3.1.0",
        signals: &["solid-js"],
    },
    FrameworkPreset {
        name: "svelte",
        package: "@santi020k/eslint-config-svelte",
        version: "3.1.0",
        signals: &["svelte"],
    },
    FrameworkPreset {
        name: "tanstack-start",
        package: "@santi020k/eslint-config-tanstack-start",
        version: "3.1.0",
        signals: &["@tanstack/react-start", "@tanstack/solid-start"],
    },
    FrameworkPreset {
        name: "vite",
        package: "@santi020k/eslint-config-vite",
        version: "3.1.0",
        signals: &["vite"],
    },
    FrameworkPreset {
        name: "vue",
        package: "@santi020k/eslint-config-vue",
        version: "3.1.0",
        signals: &["vue"],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileOwnership {
    Replace,
    Merge,
    Metadata,
}

#[derive(Debug)]
struct PresetFile {
    path: PathBuf,
    contents: String,
    ownership: FileOwnership,
}

#[derive(Debug)]
struct PresetPlan {
    profile: PresetProfile,
    ecosystems: BTreeSet<PresetEcosystem>,
    files: Vec<PresetFile>,
    dependencies: BTreeMap<&'static str, &'static str>,
}

#[derive(Clone, Copy)]
struct FrameworkPreset {
    name: &'static str,
    package: &'static str,
    version: &'static str,
    signals: &'static [&'static str],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresetMetadata {
    #[serde(rename = "$schema", default = "preset_schema")]
    schema: String,
    schema_version: u32,
    catalog_version: u32,
    profile: PresetProfile,
    ecosystems: BTreeSet<PresetEcosystem>,
    gate: GateProfile,
    managed_files: BTreeMap<String, String>,
    dependencies: BTreeMap<String, String>,
}

fn preset_schema() -> String {
    PRESET_SCHEMA.to_owned()
}

#[derive(Clone, Debug, Serialize)]
pub struct PresetDoctorStatus {
    pub profile: String,
    pub catalog_version: u32,
    pub current_catalog_version: u32,
    pub state: String,
    pub issues: Vec<String>,
}

pub fn doctor_status(project: &Project) -> Option<PresetDoctorStatus> {
    if !project.root.join(PRESET_METADATA).exists() {
        return None;
    }
    let metadata = match load_metadata_optional(&project.root) {
        Ok(Some(metadata)) => metadata,
        Ok(None) => return None,
        Err(error) => {
            return Some(PresetDoctorStatus {
                profile: "unknown".to_owned(),
                catalog_version: 0,
                current_catalog_version: PRESET_CATALOG_VERSION,
                state: "incompatible".to_owned(),
                issues: vec![format!("{error:#}")],
            });
        }
    };
    if metadata.catalog_version > PRESET_CATALOG_VERSION {
        return Some(PresetDoctorStatus {
            profile: metadata.profile.to_string(),
            catalog_version: metadata.catalog_version,
            current_catalog_version: PRESET_CATALOG_VERSION,
            state: "incompatible".to_owned(),
            issues: vec!["Preset was created by a newer quality release.".to_owned()],
        });
    }
    let ecosystems = metadata.ecosystems.iter().copied().collect::<Vec<_>>();
    let changes = build_plan(project, metadata.profile, &ecosystems, metadata.gate)
        .and_then(|plan| collect_diff(project, &plan, Some(&metadata)));
    match changes {
        Ok(changes) => PresetDoctorStatus {
            profile: metadata.profile.to_string(),
            catalog_version: metadata.catalog_version,
            current_catalog_version: PRESET_CATALOG_VERSION,
            state: if changes.is_empty() {
                "current".to_owned()
            } else {
                "update-available".to_owned()
            },
            issues: changes,
        },
        Err(error) => PresetDoctorStatus {
            profile: metadata.profile.to_string(),
            catalog_version: metadata.catalog_version,
            current_catalog_version: PRESET_CATALOG_VERSION,
            state: "incompatible".to_owned(),
            issues: vec![format!("{error:#}")],
        },
    }
    .into()
}

pub fn print_list() {
    println!("minimal      Essential analyzers with low-ceremony defaults");
    println!("recommended  Balanced language-aware defaults for most repositories");
    println!("strict       Stronger limits and warning-free CI-oriented policies");
}

pub fn print_profile(profile: PresetProfile) {
    println!("Preset: {profile}");
    println!();
    match profile {
        PresetProfile::Minimal => {
            println!("Essential ecosystem analyzers and formatters only.");
            println!("ESLint: @santi020k/eslint-config-basic preset=basic, strict=false");
        }
        PresetProfile::Recommended => {
            println!("Balanced defaults plus formatting, spelling, and unused-code checks.");
            println!(
                "ESLint: @santi020k/eslint-config-basic with auto-detection and strict=recommended"
            );
        }
        PresetProfile::Strict => {
            println!("Tighter complexity/size limits and warnings promoted to failures.");
            println!(
                "ESLint: @santi020k/eslint-config-basic with auto-detection and strict=pedantic"
            );
        }
    }
    println!("Ecosystems: javascript, python, rust, swift, kotlin, github-actions");
    println!("Use `quality preset apply {profile} --dry-run` for a repository-specific preview.");
}

pub fn apply(
    project: &Project,
    profile: PresetProfile,
    only: &[PresetEcosystem],
    gate: GateProfile,
    dry_run: bool,
    force: bool,
    install: bool,
) -> Result<()> {
    let plan = build_plan(project, profile, only, gate)?;
    if plan.ecosystems.is_empty() {
        anyhow::bail!("no supported ecosystems were detected; use --only to select one explicitly");
    }

    let previous = load_metadata_optional(&project.root)?;
    let mut conflicts = conflicting_files(&project.root, &plan.files, previous.as_ref())?;
    conflicts.extend(stale_conflicts(&project.root, &plan, previous.as_ref())?);
    if !conflicts.is_empty() && !force {
        anyhow::bail!(
            "preset would replace existing files: {}; rerun with --force after reviewing --dry-run",
            conflicts.join(", ")
        );
    }
    if install && !plan.dependencies.is_empty() && !project.root.join("package.json").exists() {
        anyhow::bail!("--install requires a package.json at the repository root");
    }

    if dry_run {
        print_preview(&plan, project)?;
        return Ok(());
    }

    let (written, unchanged) = write_plan(project, &plan, previous.as_ref(), force, install)?;
    println!(
        "Applied {} preset for {} ({} written, {} unchanged).",
        plan.profile,
        ecosystem_names(&plan.ecosystems),
        written,
        unchanged
    );
    println!("Next: quality doctor && quality check");
    Ok(())
}

pub fn diff(project: &Project) -> Result<bool> {
    let metadata = require_metadata(&project.root)?;
    let ecosystems = metadata.ecosystems.iter().copied().collect::<Vec<_>>();
    let plan = build_plan(project, metadata.profile, &ecosystems, metadata.gate)?;
    let changes = collect_diff(project, &plan, Some(&metadata))?;
    if changes.is_empty() {
        println!(
            "{} preset is current (catalog {}).",
            metadata.profile, PRESET_CATALOG_VERSION
        );
        return Ok(false);
    }
    println!("Preset differences:");
    for change in &changes {
        println!("  {change}");
    }
    println!("Run `quality preset update --dry-run` to preview the generated contents.");
    Ok(true)
}

pub fn update(project: &Project, dry_run: bool, force: bool, install: bool) -> Result<()> {
    let metadata = require_metadata(&project.root)?;
    if metadata.catalog_version > PRESET_CATALOG_VERSION {
        anyhow::bail!(
            "preset metadata uses catalog {}, but this quality release supports {}; upgrade quality before updating",
            metadata.catalog_version,
            PRESET_CATALOG_VERSION
        );
    }
    let ecosystems = metadata.ecosystems.iter().copied().collect::<Vec<_>>();
    let plan = build_plan(project, metadata.profile, &ecosystems, metadata.gate)?;
    let mut conflicts = conflicting_files(&project.root, &plan.files, Some(&metadata))?;
    conflicts.extend(stale_conflicts(&project.root, &plan, Some(&metadata))?);
    if !conflicts.is_empty() && !force {
        anyhow::bail!(
            "preset-managed files contain user changes: {}; rerun with --force only if those edits may be replaced",
            conflicts.join(", ")
        );
    }
    if dry_run {
        let changes = collect_diff(project, &plan, Some(&metadata))?;
        if changes.is_empty() {
            println!("Preset is already current.");
        } else {
            for change in changes {
                println!("{change}");
            }
            print_preview(&plan, project)?;
        }
        return Ok(());
    }
    let (written, unchanged) = write_plan(project, &plan, Some(&metadata), force, install)?;
    println!(
        "Updated {} preset to catalog {} ({} written, {} unchanged).",
        plan.profile, PRESET_CATALOG_VERSION, written, unchanged
    );
    Ok(())
}

pub fn setup(project: &Project, install: bool) -> Result<()> {
    let metadata = load_metadata_optional(&project.root)?;
    let (profile, ecosystems, gate) = metadata.map_or_else(
        || {
            (
                PresetProfile::Recommended,
                detect_ecosystems(project),
                GateProfile::Auto,
            )
        },
        |metadata| (metadata.profile, metadata.ecosystems, metadata.gate),
    );
    if ecosystems.is_empty() {
        anyhow::bail!("no supported ecosystems were detected");
    }
    let selected = ecosystems.iter().copied().collect::<Vec<_>>();
    let plan = build_plan(project, profile, &selected, gate)?;
    println!("Setup for {profile}: {}", ecosystem_names(&ecosystems));

    let dependencies = dependencies_requiring_update(project, &plan.dependencies)?;
    if !dependencies.is_empty() {
        let command = dependency_command(project, &dependencies);
        setup_step(project, "JavaScript development tools", &command, install)?;
    }
    if ecosystems.contains(&PresetEcosystem::Rust) {
        setup_step(
            project,
            "Rustfmt and Clippy components",
            &[
                "rustup".to_owned(),
                "component".to_owned(),
                "add".to_owned(),
                "rustfmt".to_owned(),
                "clippy".to_owned(),
            ],
            install,
        )?;
    }
    if ecosystems.contains(&PresetEcosystem::Swift) {
        if cfg!(target_os = "macos") {
            setup_step(
                project,
                "SwiftLint and SwiftFormat",
                &[
                    "brew".to_owned(),
                    "install".to_owned(),
                    "swiftlint".to_owned(),
                    "swiftformat".to_owned(),
                ],
                install,
            )?;
        } else {
            println!(
                "  Manual: configure SwiftLint and SwiftFormat SwiftPM plugins for this project"
            );
        }
    }
    if ecosystems.contains(&PresetEcosystem::Kotlin) {
        if cfg!(target_os = "macos") {
            setup_step(
                project,
                "detekt and ktlint",
                &[
                    "brew".to_owned(),
                    "install".to_owned(),
                    "detekt".to_owned(),
                    "ktlint".to_owned(),
                ],
                install,
            )?;
        } else {
            println!("  Manual: configure the detekt and ktlint Gradle plugins");
        }
        if project.has_file("AndroidManifest.xml") {
            println!("  Verify: commit the Gradle wrapper and ensure `java -version` succeeds");
        }
    }
    if ecosystems.contains(&PresetEcosystem::GithubActions) && which::which("actionlint").is_err() {
        setup_step(
            project,
            "Actionlint",
            &[
                "go".to_owned(),
                "install".to_owned(),
                "github.com/rhysd/actionlint/cmd/actionlint@v1.7.12".to_owned(),
            ],
            install,
        )?;
    }
    match selected_spelling_adapter(profile, &ecosystems) {
        Some("codespell") if which::which("codespell").is_err() => setup_step(
            project,
            "Codespell",
            &[
                "python3".to_owned(),
                "-m".to_owned(),
                "pip".to_owned(),
                "install".to_owned(),
                "--user".to_owned(),
                format!("codespell=={CODESPELL_VERSION}"),
            ],
            install,
        )?,
        Some("typos") if which::which("typos").is_err() => {
            let command = if cfg!(target_os = "macos") {
                vec![
                    "brew".to_owned(),
                    "install".to_owned(),
                    "typos-cli".to_owned(),
                ]
            } else {
                vec![
                    "cargo".to_owned(),
                    "install".to_owned(),
                    "typos-cli".to_owned(),
                    "--version".to_owned(),
                    TYPOS_VERSION.to_owned(),
                    "--locked".to_owned(),
                ]
            };
            setup_step(project, "Typos", &command, install)?;
        }
        _ => {}
    }
    if !install {
        println!("Run `quality preset setup --install` to execute supported setup commands.");
    }
    Ok(())
}

fn setup_step(project: &Project, name: &str, command: &[String], install: bool) -> Result<()> {
    println!("  {name}: {}", command.join(" "));
    if !install {
        return Ok(());
    }
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&project.root)
        .status()
        .with_context(|| format!("could not run setup command `{}`", command[0]))?;
    if !status.success() {
        anyhow::bail!("setup command for {name} failed with {status}");
    }
    Ok(())
}

fn build_plan(
    project: &Project,
    profile: PresetProfile,
    only: &[PresetEcosystem],
    gate: GateProfile,
) -> Result<PresetPlan> {
    let ecosystems = if only.is_empty() {
        detect_ecosystems(project)
    } else {
        only.iter().copied().collect()
    };
    let mut files = Vec::new();
    let mut tools = BTreeSet::new();
    let mut dependencies = BTreeMap::new();

    if ecosystems.contains(&PresetEcosystem::JavaScript) {
        let typescript = has_typescript(project);
        let frameworks = if profile == PresetProfile::Minimal {
            Vec::new()
        } else {
            detected_frameworks(project)?
        };
        files.push(PresetFile {
            path: PathBuf::from("eslint.config.mjs"),
            contents: eslint_config(
                profile,
                typescript,
                has_typescript_project(project),
                &frameworks,
            ),
            ownership: FileOwnership::Replace,
        });
        tools.insert("eslint".to_owned());
        dependencies.insert("@santi020k/eslint-config-basic", ESLINT_CONFIG_VERSION);
        dependencies.insert("eslint", ESLINT_VERSION);
        if profile == PresetProfile::Strict {
            dependencies.insert(
                "@santi020k/eslint-config-extensions",
                ESLINT_EXTENSIONS_VERSION,
            );
        }
        if typescript {
            dependencies.insert("typescript", TYPESCRIPT_VERSION);
        }
        for framework in &frameworks {
            dependencies.insert(framework.package, framework.version);
        }
        if has_astro(project) {
            tools.insert("astro-check".to_owned());
        }
        if profile != PresetProfile::Minimal {
            files.extend(javascript_support_files(profile));
            dependencies.insert("prettier", PRETTIER_VERSION);
            dependencies.insert("knip", KNIP_VERSION);
            tools.extend(["prettier", "knip"].map(str::to_owned));
        }
    }

    if ecosystems.contains(&PresetEcosystem::Rust) {
        files.push(PresetFile {
            path: PathBuf::from("rustfmt.toml"),
            contents: rustfmt_config(profile).to_owned(),
            ownership: FileOwnership::Replace,
        });
        files.push(PresetFile {
            path: PathBuf::from(".clippy.toml"),
            contents: clippy_config(profile).to_owned(),
            ownership: FileOwnership::Replace,
        });
        tools.extend(["cargo-fmt", "cargo-clippy"].map(str::to_owned));
    }

    if ecosystems.contains(&PresetEcosystem::Swift) {
        files.push(PresetFile {
            path: PathBuf::from(".swiftlint.yml"),
            contents: swiftlint_config(profile).to_owned(),
            ownership: FileOwnership::Replace,
        });
        files.push(PresetFile {
            path: PathBuf::from(".swiftformat"),
            contents: swiftformat_config(profile).to_owned(),
            ownership: FileOwnership::Replace,
        });
        tools.extend(["swiftlint", "swiftformat"].map(str::to_owned));
    }

    if ecosystems.contains(&PresetEcosystem::Kotlin) {
        files.push(PresetFile {
            path: PathBuf::from("detekt.yml"),
            contents: detekt_config(profile).to_owned(),
            ownership: FileOwnership::Replace,
        });
        files.push(PresetFile {
            path: PathBuf::from(".editorconfig"),
            contents: merge_managed_block(
                fs::read_to_string(project.root.join(".editorconfig"))
                    .ok()
                    .as_deref(),
                ktlint_config(profile),
            )?,
            ownership: FileOwnership::Merge,
        });
        tools.extend(["detekt", "ktlint"].map(str::to_owned));
        if project.has_file("AndroidManifest.xml") {
            tools.insert("android-lint".to_owned());
        }
    }

    if ecosystems.contains(&PresetEcosystem::GithubActions) {
        files.push(PresetFile {
            path: PathBuf::from(".github/actionlint.yaml"),
            contents: "# Generated by quality; declare repository variables here when needed.\nconfig-variables: null\n".to_owned(),
            ownership: FileOwnership::Replace,
        });
        tools.insert("actionlint".to_owned());
    }

    match selected_spelling_adapter(profile, &ecosystems) {
        Some("cspell") => {
            files.push(cspell_config());
            dependencies.insert("cspell", CSPELL_VERSION);
            tools.insert("cspell".to_owned());
        }
        Some("codespell") => {
            files.push(codespell_config());
            tools.insert("codespell".to_owned());
        }
        Some("typos") => {
            files.push(typos_config());
            tools.insert("typos".to_owned());
        }
        _ => {}
    }

    let quality = crate::config::merge_preset_text_with_gate(project, gate, &tools, profile)?;
    files.push(PresetFile {
        path: PathBuf::from("quality.yml"),
        contents: quality,
        ownership: FileOwnership::Merge,
    });
    let mut plan = PresetPlan {
        profile,
        ecosystems,
        files,
        dependencies,
    };
    attach_metadata(&mut plan, gate)?;
    plan.files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(plan)
}

fn attach_metadata(plan: &mut PresetPlan, gate: GateProfile) -> Result<()> {
    let managed_files = plan
        .files
        .iter()
        .map(|file| {
            (
                file.path.display().to_string(),
                content_fingerprint(file.contents.as_bytes()),
            )
        })
        .collect();
    let dependencies = plan
        .dependencies
        .iter()
        .map(|(name, version)| ((*name).to_owned(), (*version).to_owned()))
        .collect();
    let metadata = PresetMetadata {
        schema: PRESET_SCHEMA.to_owned(),
        schema_version: PRESET_SCHEMA_VERSION,
        catalog_version: PRESET_CATALOG_VERSION,
        profile: plan.profile,
        ecosystems: plan.ecosystems.clone(),
        gate,
        managed_files,
        dependencies,
    };
    let mut contents =
        serde_json::to_string_pretty(&metadata).context("could not serialize preset metadata")?;
    contents.push('\n');
    plan.files.push(PresetFile {
        path: PathBuf::from(PRESET_METADATA),
        contents,
        ownership: FileOwnership::Metadata,
    });
    Ok(())
}

fn content_fingerprint(contents: &[u8]) -> String {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in contents {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{value:016x}")
}

fn detect_ecosystems(project: &Project) -> BTreeSet<PresetEcosystem> {
    let mut ecosystems = BTreeSet::new();
    if project.has_file("package.json")
        || [
            "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "astro",
        ]
        .iter()
        .any(|extension| project.has_extension(extension))
    {
        ecosystems.insert(PresetEcosystem::JavaScript);
    }
    if project.has_file("Cargo.toml") || project.has_extension("rs") {
        ecosystems.insert(PresetEcosystem::Rust);
    }
    if project.has_file("pyproject.toml") || project.has_extension("py") {
        ecosystems.insert(PresetEcosystem::Python);
    }
    if project.has_file("Package.swift")
        || project.has_extension("swift")
        || project.has_directory_extension("xcodeproj")
    {
        ecosystems.insert(PresetEcosystem::Swift);
    }
    if project.has_extension("kt")
        || project.has_extension("kts")
        || project.has_file("AndroidManifest.xml")
    {
        ecosystems.insert(PresetEcosystem::Kotlin);
    }
    if project.path_contains(".github/workflows/") {
        ecosystems.insert(PresetEcosystem::GithubActions);
    }
    ecosystems
}

fn has_typescript(project: &Project) -> bool {
    ["ts", "tsx", "mts", "cts"]
        .iter()
        .any(|extension| project.has_extension(extension))
        || project.has_file("tsconfig.json")
        || project.has_file("tsconfig.base.json")
}

fn has_typescript_project(project: &Project) -> bool {
    project.has_file("tsconfig.json") || project.has_file("tsconfig.base.json")
}

fn has_astro(project: &Project) -> bool {
    project.has_extension("astro")
        || [
            "astro.config.js",
            "astro.config.mjs",
            "astro.config.cjs",
            "astro.config.ts",
            "astro.config.mts",
        ]
        .iter()
        .any(|name| project.has_file(name))
}

fn detected_frameworks(project: &Project) -> Result<Vec<FrameworkPreset>> {
    let dependencies = all_package_dependencies(project)?;
    let mut frameworks = FRAMEWORK_PRESETS
        .iter()
        .copied()
        .filter(|framework| {
            framework
                .signals
                .iter()
                .any(|signal| dependencies.contains(*signal))
        })
        .collect::<Vec<_>>();
    if has_astro(project) && !frameworks.iter().any(|framework| framework.name == "astro") {
        frameworks.push(
            *FRAMEWORK_PRESETS
                .iter()
                .find(|framework| framework.name == "astro")
                .expect("Astro framework preset is defined"),
        );
    }
    frameworks.sort_by_key(|framework| framework.name);
    Ok(frameworks)
}

fn all_package_dependencies(project: &Project) -> Result<BTreeSet<String>> {
    let mut dependencies = BTreeSet::new();
    for relative in project.paths_named("package.json") {
        let path = project.root.join(relative);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let manifest: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("invalid JSON in {}", path.display()))?;
        for section in [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ] {
            if let Some(entries) = manifest.get(section).and_then(serde_json::Value::as_object) {
                dependencies.extend(entries.keys().cloned());
            }
        }
    }
    Ok(dependencies)
}

fn eslint_config(
    profile: PresetProfile,
    typescript: bool,
    typescript_project: bool,
    frameworks: &[FrameworkPreset],
) -> String {
    let options = match profile {
        PresetProfile::Minimal => format!(
            "  detection: false,\n  preset: 'basic',\n  root: import.meta.dirname,\n  strict: false,{}",
            if typescript {
                "\n  typescript: 'syntax',"
            } else {
                ""
            }
        ),
        PresetProfile::Recommended => {
            eslint_detected_options("recommended", frameworks, typescript && !typescript_project)
        }
        PresetProfile::Strict => {
            eslint_detected_options("pedantic", frameworks, typescript && !typescript_project)
        }
    };
    format!(
        "import {{ defineConfig }} from '@santi020k/eslint-config-basic'\n\nexport default defineConfig({{\n{options}\n}})\n"
    )
}

fn eslint_detected_options(
    strict: &str,
    frameworks: &[FrameworkPreset],
    syntax_typescript: bool,
) -> String {
    let framework_option = (!frameworks.is_empty()).then(|| {
        format!(
            "\n  frameworks: {{ {} }},",
            frameworks
                .iter()
                .map(|framework| format!("'{}': true", framework.name))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    format!(
        "  autoFrameworks: false,\n  detection: {{\n    extensions: false,\n    formats: false,\n    frameworks: false,\n    libraries: false,\n    projects: false,\n    testing: false,\n    tools: false,\n  }},{}\n  root: import.meta.dirname,\n  strict: '{strict}',",
        [
            framework_option.as_deref(),
            syntax_typescript.then_some("\n  typescript: 'syntax',"),
        ]
        .into_iter()
        .flatten()
        .collect::<String>()
    )
}

fn javascript_support_files(profile: PresetProfile) -> Vec<PresetFile> {
    let print_width = if profile == PresetProfile::Strict {
        90
    } else {
        100
    };
    vec![
        PresetFile {
            path: PathBuf::from("prettier.config.mjs"),
            contents: format!(
                "export default {{\n  endOfLine: 'lf',\n  printWidth: {print_width},\n  semi: false,\n  singleQuote: true,\n  trailingComma: 'none'\n}}\n"
            ),
            ownership: FileOwnership::Replace,
        },
        PresetFile {
            path: PathBuf::from("knip.json"),
            contents: "{\n  \"$schema\": \"https://unpkg.com/knip@6/schema.json\",\n  \"ignoreDependencies\": [\"cspell\", \"prettier\"]\n}\n".to_owned(),
            ownership: FileOwnership::Replace,
        },
    ]
}

fn selected_spelling_adapter(
    profile: PresetProfile,
    ecosystems: &BTreeSet<PresetEcosystem>,
) -> Option<&'static str> {
    if ecosystems.contains(&PresetEcosystem::JavaScript) {
        return (profile != PresetProfile::Minimal).then_some("cspell");
    }
    if ecosystems.contains(&PresetEcosystem::Python) {
        return Some("codespell");
    }
    (profile != PresetProfile::Minimal
        && ecosystems.iter().any(|ecosystem| {
            matches!(
                ecosystem,
                PresetEcosystem::Rust | PresetEcosystem::Swift | PresetEcosystem::Kotlin
            )
        }))
    .then_some("typos")
}

fn cspell_config() -> PresetFile {
    PresetFile {
        path: PathBuf::from("cspell.config.yaml"),
        contents: "version: '0.2'\nuseGitignore: true\nwords:\n  - knip\n  - santi\n  - unrs\nignorePaths:\n  - .quality-baseline.json\n  - '*-lock.*'\n  - bun.lock\n  - bun.lockb\n  - coverage\n  - dist\n  - node_modules\n  - package-lock.json\n  - pnpm-lock.yaml\n  - yarn.lock\n".to_owned(),
        ownership: FileOwnership::Replace,
    }
}

fn codespell_config() -> PresetFile {
    PresetFile {
        path: PathBuf::from(".codespellrc"),
        contents: "[codespell]\nquiet-level = 2\nskip = .git,.quality-baseline.json,.quality-preset.json,.venv,coverage,dist,node_modules,target,venv\n".to_owned(),
        ownership: FileOwnership::Replace,
    }
}

fn typos_config() -> PresetFile {
    PresetFile {
        path: PathBuf::from("_typos.toml"),
        contents: "[files]\nextend-exclude = [\".quality-baseline.json\", \".quality-preset.json\", \"coverage\", \"dist\", \"node_modules\", \"target\"]\n".to_owned(),
        ownership: FileOwnership::Replace,
    }
}

fn rustfmt_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => {
            "hard_tabs = false\nmax_width = 120\nuse_small_heuristics = \"Default\"\n"
        }
        PresetProfile::Recommended => {
            "hard_tabs = false\nmax_width = 100\nuse_small_heuristics = \"Default\"\n"
        }
        PresetProfile::Strict => {
            "hard_tabs = false\nmax_width = 90\nuse_small_heuristics = \"Default\"\n"
        }
    }
}

fn clippy_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => {
            "cognitive-complexity-threshold = 30\ntoo-many-arguments-threshold = 8\ntype-complexity-threshold = 300\n"
        }
        PresetProfile::Recommended => {
            "cognitive-complexity-threshold = 25\ntoo-many-arguments-threshold = 7\ntype-complexity-threshold = 250\n"
        }
        PresetProfile::Strict => {
            "cognitive-complexity-threshold = 20\ntoo-many-arguments-threshold = 5\ntype-complexity-threshold = 200\n"
        }
    }
}

fn swiftlint_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => {
            "excluded:\n  - .build\n  - DerivedData\n  - Pods\nreporter: xcode\n"
        }
        PresetProfile::Recommended => {
            "excluded:\n  - .build\n  - DerivedData\n  - Pods\nopt_in_rules:\n  - empty_count\n  - fatal_error_message\n  - sorted_imports\nline_length:\n  warning: 120\n  error: 160\nreporter: xcode\n"
        }
        PresetProfile::Strict => {
            "excluded:\n  - .build\n  - DerivedData\n  - Pods\nstrict: true\nopt_in_rules:\n  - empty_count\n  - fatal_error_message\n  - force_unwrapping\n  - implicitly_unwrapped_optional\n  - sorted_imports\nline_length:\n  warning: 100\n  error: 120\nreporter: xcode\n"
        }
    }
}

fn swiftformat_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => "--indent 4\n--linebreaks lf\n--semicolons never\n",
        PresetProfile::Recommended => {
            "--indent 4\n--linebreaks lf\n--maxwidth 120\n--semicolons never\n--wraparguments before-first\n"
        }
        PresetProfile::Strict => {
            "--indent 4\n--linebreaks lf\n--maxwidth 100\n--semicolons never\n--wraparguments before-first\n--wrapcollections before-first\n"
        }
    }
}

fn detekt_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => "build:\n  maxIssues: 0\nconfig:\n  validation: true\n",
        PresetProfile::Recommended => {
            "build:\n  maxIssues: 0\nconfig:\n  validation: true\n  warningsAsErrors: true\ncomplexity:\n  LongMethod:\n    active: true\n    threshold: 60\n  TooManyFunctions:\n    active: true\n    thresholdInFiles: 15\n"
        }
        PresetProfile::Strict => {
            "build:\n  maxIssues: 0\nconfig:\n  validation: true\n  warningsAsErrors: true\ncomplexity:\n  CyclomaticComplexMethod:\n    active: true\n    threshold: 12\n  LongMethod:\n    active: true\n    threshold: 40\n  TooManyFunctions:\n    active: true\n    thresholdInFiles: 10\n"
        }
    }
}

fn ktlint_config(profile: PresetProfile) -> &'static str {
    match profile {
        PresetProfile::Minimal => {
            "[*.{kt,kts}]\nindent_size = 4\nktlint_code_style = ktlint_official\nmax_line_length = 120\n"
        }
        PresetProfile::Recommended => {
            "[*.{kt,kts}]\nindent_size = 4\nktlint_code_style = ktlint_official\nmax_line_length = 100\nij_kotlin_name_count_to_use_star_import = 999\n"
        }
        PresetProfile::Strict => {
            "[*.{kt,kts}]\nindent_size = 4\nktlint_code_style = ktlint_official\nmax_line_length = 90\nij_kotlin_name_count_to_use_star_import = 999\nij_kotlin_name_count_to_use_star_import_for_members = 999\n"
        }
    }
}

fn merge_managed_block(existing: Option<&str>, managed: &str) -> Result<String> {
    let block = format!(
        "{MANAGED_START}\n{}\n{MANAGED_END}\n",
        managed.trim_end_matches('\n')
    );
    let Some(existing) = existing else {
        return Ok(format!("root = true\n\n{block}"));
    };
    match (existing.find(MANAGED_START), existing.find(MANAGED_END)) {
        (Some(start), Some(end)) if start < end => {
            let suffix = end + MANAGED_END.len();
            let mut merged = String::with_capacity(existing.len() + block.len());
            merged.push_str(&existing[..start]);
            merged.push_str(&block);
            merged.push_str(existing[suffix..].trim_start_matches('\n'));
            Ok(merged)
        }
        (None, None) => Ok(format!("{}\n\n{block}", existing.trim_end())),
        _ => anyhow::bail!(
            ".editorconfig contains an incomplete quality preset managed block; restore both markers before updating"
        ),
    }
}

fn conflicting_files(
    root: &Path,
    files: &[PresetFile],
    previous: Option<&PresetMetadata>,
) -> Result<Vec<String>> {
    let mut conflicts = Vec::new();
    for file in files {
        let path = root.join(&file.path);
        if file.ownership != FileOwnership::Replace || !path.exists() {
            continue;
        }
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        if existing == file.contents {
            continue;
        }
        let path_name = file.path.display().to_string();
        let still_managed = previous
            .and_then(|metadata| metadata.managed_files.get(&path_name))
            .is_some_and(|fingerprint| fingerprint == &content_fingerprint(existing.as_bytes()));
        if !still_managed {
            conflicts.push(path_name);
        }
    }
    Ok(conflicts)
}

fn stale_conflicts(
    root: &Path,
    plan: &PresetPlan,
    previous: Option<&PresetMetadata>,
) -> Result<Vec<String>> {
    let Some(previous) = previous else {
        return Ok(Vec::new());
    };
    let current = plan
        .files
        .iter()
        .map(|file| file.path.display().to_string())
        .collect::<BTreeSet<_>>();
    let mut conflicts = Vec::new();
    for (path_name, fingerprint) in &previous.managed_files {
        if current.contains(path_name) || is_merged_path(path_name) {
            continue;
        }
        let path = root.join(path_name);
        if !path.exists() {
            continue;
        }
        let contents =
            fs::read(&path).with_context(|| format!("could not read {}", path.display()))?;
        if &content_fingerprint(&contents) != fingerprint {
            conflicts.push(path_name.clone());
        }
    }
    Ok(conflicts)
}

fn is_merged_path(path: &str) -> bool {
    matches!(path, "quality.yml" | ".editorconfig")
}

fn collect_diff(
    project: &Project,
    plan: &PresetPlan,
    previous: Option<&PresetMetadata>,
) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    for file in &plan.files {
        let path = project.root.join(&file.path);
        let label = file.path.display();
        if !path.exists() {
            changes.push(format!("A {label}"));
        } else if fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?
            != file.contents
        {
            changes.push(format!("M {label}"));
        }
    }
    if let Some(previous) = previous {
        let current = plan
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<BTreeSet<_>>();
        for path in previous.managed_files.keys() {
            if !current.contains(path) && !is_merged_path(path) && project.root.join(path).exists()
            {
                changes.push(format!("D {path}"));
            }
        }
        if previous.catalog_version != PRESET_CATALOG_VERSION {
            changes.push(format!(
                "C catalog {} -> {}",
                previous.catalog_version, PRESET_CATALOG_VERSION
            ));
        }
    }
    for spec in dependencies_requiring_update(project, &plan.dependencies)? {
        changes.push(format!("P {spec}"));
    }
    changes.sort();
    changes.dedup();
    Ok(changes)
}

fn write_plan(
    project: &Project,
    plan: &PresetPlan,
    previous: Option<&PresetMetadata>,
    force: bool,
    install: bool,
) -> Result<(usize, usize)> {
    let mut written = 0;
    let mut unchanged = 0;
    for file in &plan.files {
        let path = project.root.join(&file.path);
        if fs::read_to_string(&path).ok().as_deref() == Some(file.contents.as_str()) {
            unchanged += 1;
            continue;
        }
        crate::atomic::write(&path, file.contents.as_bytes())?;
        println!("Created {}", file.path.display());
        written += 1;
    }
    if let Some(previous) = previous {
        let current = plan
            .files
            .iter()
            .map(|file| file.path.display().to_string())
            .collect::<BTreeSet<_>>();
        for (path_name, fingerprint) in &previous.managed_files {
            if current.contains(path_name) || is_merged_path(path_name) {
                continue;
            }
            let path = project.root.join(path_name);
            if !path.exists() {
                continue;
            }
            let contents = fs::read(&path)?;
            if force || &content_fingerprint(&contents) == fingerprint {
                fs::remove_file(&path)
                    .with_context(|| format!("could not remove {}", path.display()))?;
                println!("Removed {path_name}");
            }
        }
    }
    let dependencies = dependencies_requiring_update(project, &plan.dependencies)?;
    if !dependencies.is_empty() {
        let command = dependency_command(project, &dependencies);
        if install {
            run_dependency_install(project, &command)?;
        } else {
            println!("Dependencies: {}", command.join(" "));
        }
    }
    Ok((written, unchanged))
}

pub fn load_metadata_optional(root: &Path) -> Result<Option<PresetMetadata>> {
    let path = root.join(PRESET_METADATA);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    let metadata: PresetMetadata = serde_json::from_str(&text)
        .with_context(|| format!("invalid preset metadata in {}", path.display()))?;
    if metadata.schema_version != PRESET_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported preset metadata schema {}; expected {}",
            metadata.schema_version,
            PRESET_SCHEMA_VERSION
        );
    }
    for managed in metadata.managed_files.keys() {
        let path = Path::new(managed);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            anyhow::bail!("preset metadata contains unsafe managed path `{managed}`");
        }
    }
    Ok(Some(metadata))
}

fn require_metadata(root: &Path) -> Result<PresetMetadata> {
    load_metadata_optional(root)?.ok_or_else(|| {
        anyhow::anyhow!(
            "no applied preset metadata found; run `quality preset apply recommended` first"
        )
    })
}

fn print_preview(plan: &PresetPlan, project: &Project) -> Result<()> {
    println!("Preset: {}", plan.profile);
    println!("Ecosystems: {}", ecosystem_names(&plan.ecosystems));
    let missing = dependencies_requiring_update(project, &plan.dependencies)?;
    if !missing.is_empty() {
        println!(
            "Dependencies: {}",
            dependency_command(project, &missing).join(" ")
        );
    }
    for file in &plan.files {
        println!("\n--- {}\n{}", file.path.display(), file.contents);
    }
    Ok(())
}

fn ecosystem_names(ecosystems: &BTreeSet<PresetEcosystem>) -> String {
    ecosystems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn dependencies_requiring_update(
    project: &Project,
    dependencies: &BTreeMap<&'static str, &'static str>,
) -> Result<Vec<String>> {
    let path = project.root.join("package.json");
    let manifest = if path.exists() {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .with_context(|| format!("invalid JSON in {}", path.display()))?
    } else {
        serde_json::Value::Null
    };
    Ok(dependencies
        .iter()
        .filter(|(name, version)| {
            manifest_dependency_version(&manifest, name).as_deref() != Some(*version)
        })
        .map(|(name, version)| format!("{name}@{version}"))
        .collect())
}

fn manifest_dependency_version(manifest: &serde_json::Value, name: &str) -> Option<String> {
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .find_map(|section| {
        manifest
            .get(section)
            .and_then(serde_json::Value::as_object)
            .and_then(|dependencies| dependencies.get(name))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
}

fn dependency_command(project: &Project, dependencies: &[String]) -> Vec<String> {
    let manager = package_manager(project);
    let mut command = match manager {
        "pnpm" => vec!["pnpm", "add", "--save-dev", "--save-exact"],
        "yarn" => vec!["yarn", "add", "--dev", "--exact"],
        "bun" => vec!["bun", "add", "--dev", "--exact"],
        _ => vec!["npm", "install", "--save-dev", "--save-exact"],
    }
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    command.extend(dependencies.iter().cloned());
    command
}

fn package_manager(project: &Project) -> &str {
    let declared = fs::read_to_string(project.root.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|manifest| {
            manifest
                .get("packageManager")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.split('@').next())
                .map(str::to_owned)
        });
    if let Some(manager) = declared
        .as_deref()
        .filter(|manager| matches!(*manager, "pnpm" | "yarn" | "npm" | "bun"))
    {
        return match manager {
            "pnpm" => "pnpm",
            "yarn" => "yarn",
            "bun" => "bun",
            _ => "npm",
        };
    }
    if project.root.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if project.root.join("yarn.lock").exists() {
        "yarn"
    } else if project.root.join("bun.lock").exists() || project.root.join("bun.lockb").exists() {
        "bun"
    } else {
        "npm"
    }
}

fn run_dependency_install(project: &Project, command: &[String]) -> Result<()> {
    println!("Installing dependencies: {}", command.join(" "));
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(&project.root)
        .status()
        .with_context(|| format!("could not run {}", command[0]))?;
    if !status.success() {
        anyhow::bail!("dependency installation failed with {status}");
    }
    Ok(())
}
