use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::config::{Config, HookStepConfig};

const MARKER: &str = "# Managed by quality. Do not edit.";

pub fn install(root: &Path, config: &Config) -> Result<()> {
    if config.hooks.is_empty() {
        anyhow::bail!("quality.yml does not configure any hooks");
    }
    if let Some(path) = configured_hooks_path(root)? {
        anyhow::bail!(
            "Git core.hooksPath is set to `{path}`; remove the existing hook manager before installing quality hooks"
        );
    }
    let directory = hooks_directory(root)?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    for event in config.hooks.keys() {
        let path = directory.join(event);
        if path.exists() && !is_managed(&path)? {
            anyhow::bail!(
                "{} already exists and is not managed by quality; installation stopped",
                path.display()
            );
        }
    }
    let mut installed: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    for event in config.hooks.keys() {
        let path = directory.join(event);
        let previous = path.exists().then(|| fs::read(&path)).transpose()?;
        if let Err(error) = write_launcher(&path, event) {
            for (installed_path, previous) in installed.into_iter().rev() {
                match previous {
                    Some(contents) => {
                        let _ = crate::atomic::write_executable(&installed_path, &contents);
                    }
                    None => {
                        let _ = fs::remove_file(&installed_path);
                    }
                }
            }
            return Err(error).with_context(|| {
                format!(
                    "hook installation rolled back after {} failed",
                    path.display()
                )
            });
        }
        installed.push((path, previous));
        println!("Installed {event}");
    }
    Ok(())
}

pub fn status(root: &Path, config: &Config) -> Result<()> {
    if config.hooks.is_empty() {
        println!("No hooks are configured in quality.yml.");
        return Ok(());
    }
    if let Some(path) = configured_hooks_path(root)? {
        anyhow::bail!(
            "Git core.hooksPath is set to `{path}`; quality's managed launchers are not active"
        );
    }
    let directory = hooks_directory(root)?;
    let mut missing = Vec::new();
    for event in config.hooks.keys() {
        let path = directory.join(event);
        if path.exists() && is_managed(&path)? {
            println!("  ✓ {event}");
        } else {
            println!("  ✗ {event} (not installed)");
            missing.push(event);
        }
    }
    if !missing.is_empty() {
        anyhow::bail!("{} configured hook(s) are not installed", missing.len());
    }
    Ok(())
}

pub fn uninstall(root: &Path, config: &Config) -> Result<()> {
    let directory = hooks_directory(root)?;
    for event in config.hooks.keys() {
        let path = directory.join(event);
        if path.exists() && is_managed(&path)? {
            fs::remove_file(&path)
                .with_context(|| format!("could not remove {}", path.display()))?;
            println!("Removed {event}");
        }
    }
    Ok(())
}

pub fn run(root: &Path, config: &Config, event: &str, hook_args: &[OsString]) -> Result<()> {
    let hook = config
        .hooks
        .get(event)
        .with_context(|| format!("hook `{event}` is not configured in quality.yml"))?;
    for (index, step) in hook.steps.iter().enumerate() {
        let name = step
            .name
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} {}", step.command.display(), step.args.join(" ")));
        println!("[{event}] {name}");
        run_step(root, step, hook_args)
            .with_context(|| format!("step {} (`{name}`) failed in hook `{event}`", index + 1))?;
    }
    Ok(())
}

fn run_step(root: &Path, step: &HookStepConfig, hook_args: &[OsString]) -> Result<()> {
    let directory = step
        .working_directory
        .as_ref()
        .map_or_else(|| root.to_path_buf(), |path| root.join(path));
    let mut command = Command::new(&step.command);
    command.args(&step.args).current_dir(directory);
    if step.pass_hook_args {
        command.args(hook_args);
    }
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    let status = command
        .status()
        .with_context(|| format!("could not run `{}`", step.command.display()))?;
    if !status.success() {
        anyhow::bail!("command exited with {status}");
    }
    Ok(())
}

fn configured_hooks_path(root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["config", "--local", "--get", "core.hooksPath"])
        .current_dir(root)
        .output()
        .context("could not inspect Git hook configuration")?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }
    Ok(None)
}

fn hooks_directory(root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(root)
        .output()
        .context("could not locate the Git hooks directory")?;
    if !output.status.success() {
        anyhow::bail!("{} is not a Git repository", root.display());
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn is_managed(path: &Path) -> Result<bool> {
    let text =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    Ok(text.lines().any(|line| line == MARKER))
}

fn write_launcher(path: &Path, event: &str) -> Result<()> {
    let script = format!(
        "#!/bin/sh\n{MARKER}\nexec quality --root \"$(git rev-parse --show-toplevel)\" hooks run {event} \"$@\"\n"
    );
    crate::atomic::write_executable(path, script.as_bytes())
        .with_context(|| format!("could not write {}", path.display()))
}
