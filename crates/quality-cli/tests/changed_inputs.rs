use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// Fixture processes must not inherit the repository or recursion guard of a
// parent Git/Quality hook. Tests can still set an explicit override afterwards.
fn isolated_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    for variable in [
        "QUALITY_LOCAL_CI",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
    ] {
        command.env_remove(variable);
    }
    command
}

struct Repository {
    directory: tempfile::TempDir,
}

impl Repository {
    fn new() -> Self {
        let repository = Self {
            directory: tempfile::tempdir().expect("create test repository"),
        };
        repository.git(&["init", "--quiet"]);
        repository.git(&["config", "user.email", "quality@example.test"]);
        repository.git(&["config", "user.name", "Quality Tests"]);
        repository.git(&["config", "commit.gpgSign", "false"]);
        // Keep rename detection enabled to prove discovery does not depend on
        // the user's Git configuration or its rename similarity threshold.
        repository.git(&["config", "diff.renames", "true"]);
        repository
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.root().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture directory");
        }
        fs::write(path, contents).expect("write fixture");
    }

    fn git(&self, args: &[&str]) -> Output {
        let output = isolated_command("git")
            .arg("-C")
            .arg(self.root())
            .args(args)
            .output()
            .expect("run Git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", "fixture"]);
    }

    fn preview(&self, root: &Path, base: &str) -> String {
        let output = isolated_command(env!("CARGO_BIN_EXE_quality"))
            .arg("--root")
            .arg(root)
            .args(["check", "--changed", base, "--dry-run", "--explain"])
            .output()
            .expect("run Quality");
        assert!(
            output.status.success(),
            "Quality failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 check preview")
    }

    fn configuration_task(&self, directory: &str) -> PathBuf {
        let directory = Path::new(directory);
        self.write(
            &directory.join("quality.yml").to_string_lossy(),
            "version: 1\ntasks:\n  configuration-check:\n    command: echo\n    args: [checked]\n    config_files: [rules.txt]\n",
        );
        self.write(
            &directory.join("rules.txt").to_string_lossy(),
            "analyzer configuration\n",
        );
        self.root().join(directory)
    }
}

fn assert_configuration_rename_runs(preview: &str) {
    assert!(preview.contains("\"rules.txt\" (deleted)"), "{preview}");
    assert!(preview.contains("\"archived.txt\""), "{preview}");
    assert!(preview.contains("\nRUN configuration-check\n"), "{preview}");
    assert!(preview.contains("1 check command(s) planned."), "{preview}");
}

#[test]
fn explicit_root_ignores_inherited_git_paths_and_preserves_git_configuration() {
    let inherited = Repository::new();
    inherited.configuration_task("");
    inherited.write("leaked.ts", "original\n");
    inherited.commit();
    inherited.write("leaked.ts", "changed in the other repository\n");
    inherited.write("global-ignore", "ignored.ts\n");

    let selected = Repository::new();
    let root = selected.configuration_task("");
    selected.commit();
    selected.git(&["mv", "rules.txt", "archived.txt"]);
    selected.write(
        "ignored.ts",
        "untracked source excluded by Git configuration\n",
    );

    let output = isolated_command(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(&root)
        .args(["check", "--changed", "HEAD", "--dry-run", "--explain"])
        .env("GIT_DIR", inherited.root().join(".git"))
        .env("GIT_WORK_TREE", inherited.root())
        .env("GIT_INDEX_FILE", inherited.root().join(".git/index"))
        .env("GIT_COMMON_DIR", inherited.root().join(".git"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.excludesFile")
        .env("GIT_CONFIG_VALUE_0", inherited.root().join("global-ignore"))
        .output()
        .expect("run Quality with inherited hook repository paths");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8(output.stdout).unwrap();
    assert_configuration_rename_runs(&preview);
    assert!(!preview.contains("leaked.ts"), "{preview}");
    assert!(!preview.contains("ignored.ts"), "{preview}");
}

#[test]
fn missing_base_explains_how_to_fetch_ci_history_and_remains_an_error() {
    let repository = Repository::new();
    repository.configuration_task("");
    repository.commit();
    let output = isolated_command(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(repository.root())
        .args(["check", "--changed", "origin/unavailable"])
        .output()
        .expect("run Quality with an unavailable base");
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("origin/unavailable"), "{error}");
    assert!(error.contains("git fetch"), "{error}");
    assert!(error.contains("fetch-depth: 0"), "{error}");
}

#[test]
fn missing_merge_base_explains_ci_history_recovery_without_passing() {
    let repository = Repository::new();
    repository.configuration_task("");
    repository.commit();
    let base = String::from_utf8(repository.git(&["rev-parse", "HEAD"]).stdout).unwrap();
    repository.git(&["checkout", "--quiet", "--orphan", "disconnected"]);
    repository.write("rules.txt", "independent history\n");
    repository.commit();
    let output = isolated_command(env!("CARGO_BIN_EXE_quality"))
        .arg("--root")
        .arg(repository.root())
        .args(["check", "--changed", base.trim()])
        .output()
        .expect("run Quality without a merge base");
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("merge base"), "{error}");
    assert!(error.contains("git fetch"), "{error}");
    assert!(error.contains("fetch-depth: 0"), "{error}");
}

#[test]
fn staged_configuration_rename_runs_the_configured_check() {
    let repository = Repository::new();
    let root = repository.configuration_task("");
    repository.commit();
    repository.git(&["mv", "rules.txt", "archived.txt"]);

    assert_configuration_rename_runs(&repository.preview(&root, "HEAD"));
}

#[test]
fn unstaged_configuration_rename_runs_the_configured_check() {
    let repository = Repository::new();
    let root = repository.configuration_task("");
    repository.commit();
    fs::rename(root.join("rules.txt"), root.join("archived.txt")).expect("rename configuration");

    assert_configuration_rename_runs(&repository.preview(&root, "HEAD"));
}

#[test]
fn committed_configuration_rename_runs_against_a_base_reference() {
    let repository = Repository::new();
    let root = repository.configuration_task("");
    repository.commit();
    repository.git(&["branch", "quality-base"]);
    repository.git(&["mv", "rules.txt", "archived.txt"]);
    repository.commit();

    assert_configuration_rename_runs(&repository.preview(&root, "quality-base"));
}

#[test]
fn local_configuration_rename_is_included_alongside_branch_changes() {
    let repository = Repository::new();
    let root = repository.configuration_task("");
    repository.commit();
    repository.git(&["branch", "quality-base"]);
    repository.write("README.md", "branch change\n");
    repository.commit();
    repository.git(&["mv", "rules.txt", "archived.txt"]);

    assert_configuration_rename_runs(&repository.preview(&root, "quality-base"));
}

#[test]
fn nested_configuration_rename_uses_project_relative_paths() {
    let repository = Repository::new();
    let root = repository.configuration_task("packages/app");
    repository.write("packages/sibling/rules.txt", "sibling configuration\n");
    repository.commit();
    repository.git(&["branch", "quality-base"]);
    repository.git(&["mv", "packages/app/rules.txt", "packages/app/archived.txt"]);
    repository.git(&[
        "mv",
        "packages/sibling/rules.txt",
        "packages/sibling/archived.txt",
    ]);

    for base in ["HEAD", "quality-base"] {
        let preview = repository.preview(&root, base);
        assert_configuration_rename_runs(&preview);
        assert!(!preview.contains("packages/app/rules.txt"), "{preview}");
        assert!(!preview.contains("sibling"), "{preview}");
    }
}

#[cfg(unix)]
#[test]
fn source_type_changes_run_the_configured_check() {
    use std::os::unix::fs::symlink;

    for initially_symlink in [false, true] {
        for staged in [false, true] {
            let repository = Repository::new();
            repository.write(
                "quality.yml",
                "version: 1\ntasks:\n  source-check:\n    command: echo\n    args: [checked]\n    extensions: [ts]\n",
            );
            repository.write("implementation.txt", "export default 1;\n");
            let source = repository.root().join("index.ts");
            if initially_symlink {
                symlink("implementation.txt", &source).expect("create source symlink");
            } else {
                repository.write("index.ts", "export default 1;\n");
            }
            repository.commit();
            repository.git(&["branch", "quality-base"]);

            fs::remove_file(&source).expect("remove previous source type");
            if initially_symlink {
                repository.write("index.ts", "export default 2;\n");
            } else {
                symlink("implementation.txt", &source).expect("replace source with symlink");
            }
            if staged {
                repository.git(&["add", "index.ts"]);
            }

            for base in ["HEAD", "quality-base"] {
                let preview = repository.preview(repository.root(), base);
                assert!(preview.contains("\"index.ts\""), "{preview}");
                assert!(preview.contains("\nRUN source-check\n"), "{preview}");
                assert!(preview.contains("1 check command(s) planned."), "{preview}");
            }
        }
    }
}
