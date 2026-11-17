use std::fs;
use std::path::Path;
use std::process::{Command, Output};

struct Repository {
    directory: tempfile::TempDir,
}

impl Repository {
    fn new(name: &str, hook: &str) -> Self {
        let repository = Self {
            directory: tempfile::tempdir().expect("create test repository"),
        };
        let output = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repository.root())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .output()
            .expect("initialize test repository");
        assert_success(&output);
        fs::write(
            repository.root().join("quality.yml"),
            format!(
                "version: 1\nhooks:\n  {hook}:\n    steps:\n      - name: {name}\n        command: git\n        args: [--version]\n"
            ),
        )
        .expect("write hook configuration");
        repository
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn quality(&self, args: &[&str], inherited_repository: Option<&Self>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_quality"));
        command
            .arg("--root")
            .arg(self.root())
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
        if let Some(inherited) = inherited_repository {
            command
                .env("GIT_DIR", inherited.root().join(".git"))
                .env("GIT_WORK_TREE", inherited.root())
                .env("GIT_COMMON_DIR", inherited.root().join(".git"));
        }
        let output = command.output().expect("run Quality");
        assert_success(&output);
        output
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn retains_history_in_explicit_root_despite_inherited_git_repository() {
    let inherited = Repository::new("Inherited repository", "pre-commit");
    let target = Repository::new("Target repository", "pre-push");
    inherited.quality(&["ci", "local", "--hook", "pre-commit"], None);
    let inherited_history = inherited.root().join(".git/quality/local-ci");
    let previous_latest = fs::read(inherited_history.join("latest.json")).unwrap();

    target.quality(&["ci", "local", "--format", "json"], Some(&inherited));

    let latest = fs::read(target.root().join(".git/quality/local-ci/latest.json"))
        .expect("retain the target repository's history in its own Git directory");
    let report: serde_json::Value = serde_json::from_slice(&latest).unwrap();
    assert_eq!(report["hook"], "pre-push");
    assert_eq!(report["steps"][0]["name"], "Target repository");
    assert_eq!(
        fs::read(inherited_history.join("latest.json")).unwrap(),
        previous_latest
    );
    assert_eq!(fs::read_dir(inherited_history).unwrap().count(), 2);
}

#[test]
fn queries_history_in_explicit_root_despite_inherited_git_repository() {
    let inherited = Repository::new("Inherited repository", "pre-commit");
    let target = Repository::new("Target repository", "pre-push");
    inherited.quality(&["ci", "local", "--hook", "pre-commit"], None);
    target.quality(&["ci", "local"], None);

    let output = target.quality(&["ci", "history", "--format", "json"], Some(&inherited));
    let history: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let runs = history["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["hook"], "pre-push");

    let output = target.quality(
        &["ci", "show", "latest", "--format", "json"],
        Some(&inherited),
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["steps"][0]["name"], "Target repository");
}
