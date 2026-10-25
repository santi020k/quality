use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ChangeSet {
    pub base: String,
    #[serde(skip)]
    pub resolved_base: String,
    pub files: Vec<PathBuf>,
    pub deleted: BTreeSet<PathBuf>,
}

pub fn discover(root: &Path, base: &str) -> Result<ChangeSet> {
    ensure_repository(root)?;
    resolve_revision(root, "HEAD").context(
        "changed-file mode needs at least one Git commit; run without `--changed` in a new repository",
    )?;
    let resolved_base = resolve_revision(root, base)
        .with_context(|| format!("Git base `{base}` does not resolve to a commit"))?;

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
            "--relative",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
            &comparison,
            "--",
            ".",
        ],
        &mut files,
    )
    .with_context(|| format!("could not compare the project with `{base}`"))?;
    let mut deleted = BTreeSet::new();
    collect_paths(
        root,
        &[
            "diff",
            "--relative",
            "--name-only",
            "-z",
            "--diff-filter=D",
            &comparison,
            "--",
            ".",
        ],
        &mut deleted,
    )
    .with_context(|| format!("could not inspect deletions since `{base}`"))?;

    if base != "HEAD" {
        collect_paths(
            root,
            &[
                "diff",
                "--relative",
                "--name-only",
                "-z",
                "--diff-filter=ACMR",
                "HEAD",
                "--",
                ".",
            ],
            &mut files,
        )
        .context("could not inspect uncommitted changes")?;
        collect_paths(
            root,
            &[
                "diff",
                "--relative",
                "--name-only",
                "-z",
                "--diff-filter=D",
                "HEAD",
                "--",
                ".",
            ],
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
        resolved_base,
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

fn resolve_revision(root: &Path, revision: &str) -> Result<String> {
    let revision = format!("{revision}^{{commit}}");
    let output = git(root, &["rev-parse", "--verify", "--quiet", &revision])?;
    if !output.status.success() {
        anyhow::bail!("revision does not exist");
    }
    Ok(String::from_utf8(output.stdout)
        .context("Git returned a non-UTF-8 commit identifier")?
        .trim()
        .to_owned())
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

    // `--root` can point at a subdirectory of a larger Git repository (for
    // example a monorepo package). `git diff --name-only` reports paths
    // relative to the repository top-level in that case, while
    // `git ls-files` reports paths relative to the `-C` directory. Mixing
    // the two coordinate systems produced paths like `app/index.js` next to
    // `new.js`, which `root.join(path)` then resolved incorrectly (and let
    // sibling-directory changes leak into the report).
    fn init_nested_repo(temp: &Path) -> (PathBuf, PathBuf) {
        let app = temp.join("app");
        let sibling = temp.join("sibling");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        run_git(temp, &["init", "--quiet"]);
        run_git(temp, &["config", "user.email", "quality@example.test"]);
        run_git(temp, &["config", "user.name", "Quality Tests"]);
        std::fs::write(app.join("index.js"), "export default 1;\n").unwrap();
        std::fs::write(sibling.join("other.js"), "export default 2;\n").unwrap();
        run_git(temp, &["add", "app/index.js", "sibling/other.js"]);
        run_git(temp, &["commit", "--quiet", "-m", "initial"]);
        (app, sibling)
    }

    #[test]
    fn nested_root_reports_tracked_modifications_relative_to_root() {
        let temp = tempfile::tempdir().unwrap();
        let (app, _sibling) = init_nested_repo(temp.path());

        std::fs::write(app.join("index.js"), "export default 3;\n").unwrap();

        let changes = discover(&app, "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("index.js")]);
    }

    #[test]
    fn nested_root_reports_deletions_relative_to_root() {
        let temp = tempfile::tempdir().unwrap();
        let (app, _sibling) = init_nested_repo(temp.path());

        std::fs::remove_file(app.join("index.js")).unwrap();

        let changes = discover(&app, "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("index.js")]);
        assert!(changes.is_deleted(Path::new("index.js")));
    }

    #[test]
    fn nested_root_reports_untracked_paths_relative_to_root() {
        let temp = tempfile::tempdir().unwrap();
        let (app, _sibling) = init_nested_repo(temp.path());

        std::fs::write(app.join("new.js"), "export default 4;\n").unwrap();

        let changes = discover(&app, "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("new.js")]);
    }

    #[test]
    fn nested_root_excludes_sibling_directory_changes() {
        let temp = tempfile::tempdir().unwrap();
        let (app, sibling) = init_nested_repo(temp.path());

        std::fs::write(app.join("index.js"), "export default 3;\n").unwrap();
        std::fs::write(sibling.join("other.js"), "export default 5;\n").unwrap();
        std::fs::write(sibling.join("untracked.js"), "export default 6;\n").unwrap();

        let changes = discover(&app, "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("index.js")]);
    }

    #[test]
    fn nested_root_reports_non_head_base_relative_to_root() {
        let temp = tempfile::tempdir().unwrap();
        let (app, sibling) = init_nested_repo(temp.path());
        run_git(temp.path(), &["branch", "base"]);

        std::fs::write(app.join("index.js"), "export default 3;\n").unwrap();
        std::fs::write(sibling.join("other.js"), "export default 5;\n").unwrap();
        run_git(temp.path(), &["add", "app/index.js", "sibling/other.js"]);
        run_git(temp.path(), &["commit", "--quiet", "-m", "second"]);

        let changes = discover(&app, "base").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("index.js")]);
    }
    #[test]
    fn nested_root_respects_relative_diff_configuration_without_double_stripping() {
        let temp = tempfile::tempdir().unwrap();
        let (app, _) = init_nested_repo(temp.path());
        run_git(temp.path(), &["config", "diff.relative", "true"]);
        std::fs::create_dir(app.join("app")).unwrap();
        std::fs::write(app.join("app/nested.js"), "original\n").unwrap();
        run_git(temp.path(), &["add", "app/app/nested.js"]);
        run_git(temp.path(), &["commit", "--quiet", "-m", "nested"]);
        std::fs::write(app.join("app/nested.js"), "modified\n").unwrap();

        let changes = discover(&app, "HEAD").unwrap();

        assert_eq!(changes.files, vec![PathBuf::from("app/nested.js")]);
    }
}
