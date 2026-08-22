use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ChangeSet {
    pub base: String,
    pub files: Vec<PathBuf>,
    pub deleted: BTreeSet<PathBuf>,
}

pub fn discover(root: &Path, base: &str) -> Result<ChangeSet> {
    ensure_repository(root)?;
    ensure_revision(root, "HEAD").context(
        "changed-file mode needs at least one Git commit; run without `--changed` in a new repository",
    )?;
    if base != "HEAD" {
        ensure_revision(root, base)
            .with_context(|| format!("Git base `{base}` does not resolve to a commit"))?;
    }

    let mut files = BTreeSet::new();
    let comparison = if base == "HEAD" {
        "HEAD".to_owned()
    } else {
        format!("{base}...HEAD")
    };
    collect_paths(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
            &comparison,
            "--",
        ],
        &mut files,
    )
    .with_context(|| format!("could not compare the project with `{base}`"))?;
    let mut deleted = BTreeSet::new();
    collect_paths(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--diff-filter=D",
            &comparison,
            "--",
        ],
        &mut deleted,
    )
    .with_context(|| format!("could not inspect deletions since `{base}`"))?;

    if base != "HEAD" {
        collect_paths(
            root,
            &[
                "diff",
                "--name-only",
                "-z",
                "--diff-filter=ACMR",
                "HEAD",
                "--",
            ],
            &mut files,
        )
        .context("could not inspect uncommitted changes")?;
        collect_paths(
            root,
            &["diff", "--name-only", "-z", "--diff-filter=D", "HEAD", "--"],
            &mut deleted,
        )
        .context("could not inspect uncommitted deletions")?;
    }
    collect_paths(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &mut files,
    )
    .context("could not inspect untracked files")?;

    deleted.retain(|path| !root.join(path).exists());
    files.extend(deleted.iter().cloned());

    Ok(ChangeSet {
        base: base.to_owned(),
        files: files.into_iter().collect(),
        deleted,
    })
}

impl ChangeSet {
    pub fn is_deleted(&self, path: &Path) -> bool {
        self.deleted.contains(path)
    }
}

fn ensure_repository(root: &Path) -> Result<()> {
    let output = git(root, &["rev-parse", "--is-inside-work-tree"])?;
    if !output.status.success() || output.stdout != b"true\n" {
        anyhow::bail!(
            "{} is not inside a Git repository; `--changed` requires Git",
            root.display()
        );
    }
    Ok(())
}

fn ensure_revision(root: &Path, revision: &str) -> Result<()> {
    let revision = format!("{revision}^{{commit}}");
    let output = git(root, &["rev-parse", "--verify", "--quiet", &revision])?;
    if !output.status.success() {
        anyhow::bail!("revision does not exist");
    }
    Ok(())
}

fn collect_paths(root: &Path, args: &[&str], paths: &mut BTreeSet<PathBuf>) -> Result<()> {
    let output = git(root, args)?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), message.trim());
    }
    for value in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
    {
        let value = String::from_utf8(value.to_vec()).context("Git returned a non-UTF-8 path")?;
        let path = PathBuf::from(value);
        if path.is_relative() {
            paths.insert(path);
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("could not run Git; install Git or run without `--changed`")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn finds_tracked_and_untracked_changes() {
        let temp = tempfile::tempdir().unwrap();
        run_git(temp.path(), &["init", "--quiet"]);
        run_git(
            temp.path(),
            &["config", "user.email", "quality@example.test"],
        );
        run_git(temp.path(), &["config", "user.name", "Quality Tests"]);
        std::fs::write(temp.path().join("App.swift"), "struct App {}\n").unwrap();
        run_git(temp.path(), &["add", "App.swift"]);
        run_git(temp.path(), &["commit", "--quiet", "-m", "initial"]);

        std::fs::write(temp.path().join("App.swift"), "struct ChangedApp {}\n").unwrap();
        std::fs::write(temp.path().join("New.kt"), "class New\n").unwrap();
        let changes = discover(temp.path(), "HEAD").unwrap();

        assert_eq!(
            changes.files,
            vec![PathBuf::from("App.swift"), PathBuf::from("New.kt")]
        );
        assert!(changes.deleted.is_empty());
    }

    #[test]
    fn includes_deleted_paths_without_treating_them_as_active() {
        let temp = tempfile::tempdir().unwrap();
        run_git(temp.path(), &["init", "--quiet"]);
        run_git(
            temp.path(),
            &["config", "user.email", "quality@example.test"],
        );
        run_git(temp.path(), &["config", "user.name", "Quality Tests"]);
        std::fs::write(temp.path().join("eslint.config.js"), "export default [];\n").unwrap();
        run_git(temp.path(), &["add", "eslint.config.js"]);
        run_git(temp.path(), &["commit", "--quiet", "-m", "initial"]);
        std::fs::remove_file(temp.path().join("eslint.config.js")).unwrap();

        let changes = discover(temp.path(), "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("eslint.config.js")]);
        assert!(changes.is_deleted(Path::new("eslint.config.js")));
    }
}
