#![cfg(unix)]

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REPOSITORY: &str = "acme/quality";
const VERSION: &str = "v0.1.0";
const RELEASED_BINARY: &str = "test release binary\n";
const TAMPERED_BINARY: &str = "tampered release binary\n";
const OTHER_ASSET: &str = "quality-x86_64-pc-windows-msvc.zip";
/// The SHA-256 of empty input, which checksum utilities report for `/dev/null`.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const CHECKSUM_UTILITIES: [&str; 2] = ["sha256sum", "shasum"];
/// Commands the installer runs besides a SHA-256 utility.
const INSTALLER_COMMANDS: [&str; 8] = [
    "curl", "install", "mkdir", "mktemp", "rm", "tar", "tr", "uname",
];
/// GNU tar decompresses through `gzip`; other tar implementations do not need it.
const OPTIONAL_COMMANDS: [&str; 1] = ["gzip"];

#[test]
fn installer_downloads_verifies_and_installs_a_release_archive() {
    let release = Release::publish();
    let Release { digest, asset, .. } = &release;
    release.write_checksums(&format!("{digest}  {asset}\n"));

    let output = release.install();

    assert_installed(&release, &output, "inherited PATH");
}

#[test]
fn installer_accepts_standard_entries_for_the_archive_with_each_checksum_utility() {
    for utility in available(&CHECKSUM_UTILITIES) {
        let release = Release::publish();
        let Release { digest, asset, .. } = &release;
        let uppercase = digest.to_uppercase();
        for checksums in [
            format!("{digest}  {asset}\n"),
            format!("{digest} *{asset}\n"),
            format!("{digest} {asset}\n"),
            format!("{uppercase}  {asset}\n"),
            format!("{digest}  {asset}\r\n"),
            format!("{digest}  {asset}"),
            format!("# Release checksums\n{EMPTY_SHA256}  {OTHER_ASSET}\n{digest}  {asset}\n"),
        ] {
            release.write_checksums(&checksums);

            let output = release.install_with(&[utility]);

            assert_installed(&release, &output, &format!("{utility} with {checksums:?}"));
            release.remove_installed_binary();
        }
    }
}

#[test]
fn installer_verifies_piped_scripts_with_each_available_shell() {
    for shell in available(&["sh", "dash", "bash"]) {
        let release = Release::publish();
        let Release { digest, asset, .. } = &release;

        release.write_checksums(&format!("{EMPTY_SHA256}  /dev/null\n"));
        let output = release.install_piped(shell);
        let case = format!("{shell} with a /dev/null entry");
        assert_rejected(&release, &output, &entry_error(asset), &case);

        release.write_checksums(&format!("{} *{asset}\r\n", digest.to_uppercase()));
        let output = release.install_piped(shell);
        let case = format!("{shell} with an uppercase Windows line ending entry");
        assert_installed(&release, &output, &case);
    }
}

#[test]
fn installer_rejects_checksum_files_without_an_entry_for_the_archive() {
    for utility in available(&CHECKSUM_UTILITIES) {
        let release = Release::publish();
        let Release { digest, asset, .. } = &release;
        for checksums in [
            format!("{EMPTY_SHA256}  /dev/null\n"),
            format!("{digest}  {OTHER_ASSET}\n"),
            format!("{digest}  ./{asset}\n"),
            format!("{digest}  {asset}.part\n"),
            "<html>Not Found</html>\n".to_owned(),
            String::new(),
        ] {
            release.write_checksums(&checksums);

            let output = release.install_with(&[utility]);

            let case = format!("{utility} with {checksums:?}");
            assert_rejected(&release, &output, &entry_error(asset), &case);
        }
    }
}

#[test]
fn installer_rejects_ambiguous_or_non_sha256_entries_for_the_archive() {
    for utility in available(&CHECKSUM_UTILITIES) {
        let release = Release::publish();
        let Release { digest, asset, .. } = &release;
        let sha1 = file_digest(&release.archive(), "1");
        for checksums in [
            format!("{sha1}  {asset}\n"),
            format!("{}  {asset}\n", &digest[1..]),
            format!("{digest}  {asset}\n{EMPTY_SHA256}  {asset}\n"),
            format!("{EMPTY_SHA256}  {asset}\n{digest}  {asset}\n"),
            format!("{digest}  {asset}\n{digest} *{asset}\n"),
        ] {
            release.write_checksums(&checksums);

            let output = release.install_with(&[utility]);

            let case = format!("{utility} with {checksums:?}");
            assert_rejected(&release, &output, &entry_error(asset), &case);
        }
    }
}

#[test]
fn installer_rejects_an_archive_that_does_not_match_its_checksum_entry() {
    for utility in available(&CHECKSUM_UTILITIES) {
        let release = Release::publish();
        let Release { digest, asset, .. } = &release;

        release.write_checksums(&format!("{EMPTY_SHA256}  {asset}\n"));
        let output = release.install_with(&[utility]);
        let case = format!("{utility} with another file's digest");
        assert_rejected(&release, &output, &mismatch_error(asset), &case);

        release.write_checksums(&format!("{digest}  {asset}\n"));
        write_archive(&release.archive(), TAMPERED_BINARY);
        let output = release.install_with(&[utility]);
        let case = format!("{utility} with a tampered archive");
        assert_rejected(&release, &output, &mismatch_error(asset), &case);
    }
}

#[test]
fn installer_requires_a_sha256_utility() {
    let release = Release::publish();
    let Release { digest, asset, .. } = &release;
    release.write_checksums(&format!("{digest}  {asset}\n"));

    let output = release.install_with(&[]);

    let error = "A SHA-256 tool (sha256sum or shasum) is required.";
    assert_rejected(&release, &output, error, "no checksum utility");
}

/// A release published below the `file://` base URL that the installer downloads from.
struct Release {
    temp: tempfile::TempDir,
    directory: PathBuf,
    asset: String,
    digest: String,
}

impl Release {
    fn publish() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp
            .path()
            .join(REPOSITORY)
            .join("releases/download")
            .join(VERSION);
        fs::create_dir_all(&directory).unwrap();
        fs::create_dir(temp.path().join("tmp")).unwrap();
        let asset = format!("quality-{}.tar.gz", current_supported_target());
        let archive = directory.join(&asset);
        write_archive(&archive, RELEASED_BINARY);
        let digest = file_digest(&archive, "256");
        Self {
            temp,
            directory,
            asset,
            digest,
        }
    }

    fn archive(&self) -> PathBuf {
        self.directory.join(&self.asset)
    }

    fn write_checksums(&self, contents: &str) {
        let checksums = self.directory.join(format!("{}.sha256", self.asset));
        fs::write(checksums, contents).unwrap();
    }

    fn installed_binary(&self) -> Option<String> {
        fs::read_to_string(self.temp.path().join("bin/quality")).ok()
    }

    fn remove_installed_binary(&self) {
        fs::remove_file(self.temp.path().join("bin/quality")).unwrap();
    }

    /// Runs the installer with the inherited PATH.
    fn install(&self) -> Output {
        self.run(
            self.installer("sh")
                .arg(script())
                .args([REPOSITORY, VERSION]),
        )
    }

    /// Pipes the installer into `shell`, as the documented installation command does.
    fn install_piped(&self, shell: &str) -> Output {
        let script = fs::File::open(script()).unwrap();
        self.run(
            self.installer(shell)
                .args(["-s", "--", REPOSITORY, VERSION])
                .stdin(script),
        )
    }

    /// Runs the installer with a PATH whose only SHA-256 utilities are `checksum_utilities`.
    fn install_with(&self, checksum_utilities: &[&str]) -> Output {
        let tools = tempfile::tempdir_in(self.temp.path()).unwrap();
        for command in INSTALLER_COMMANDS.iter().chain(checksum_utilities) {
            let path = which::which(command).unwrap_or_else(|error| panic!("{command}: {error}"));
            symlink(path, tools.path().join(command)).unwrap();
        }
        for command in OPTIONAL_COMMANDS {
            if let Ok(path) = which::which(command) {
                symlink(path, tools.path().join(command)).unwrap();
            }
        }
        let shell = which::which("sh").unwrap();
        self.run(
            self.installer(shell)
                .arg(script())
                .args([REPOSITORY, VERSION])
                .env("PATH", tools.path()),
        )
    }

    fn installer(&self, shell: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(shell);
        command
            .env(
                "QUALITY_RELEASE_BASE_URL",
                format!("file://{}", self.temp.path().display()),
            )
            .env("QUALITY_INSTALL_DIR", self.temp.path().join("bin"))
            .env("TMPDIR", self.temp.path().join("tmp"));
        command
    }

    fn run(&self, installer: &mut Command) -> Output {
        let output = installer.output().unwrap();
        let leftovers: Vec<PathBuf> = fs::read_dir(self.temp.path().join("tmp"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert!(
            leftovers.is_empty(),
            "installer left {leftovers:?}\n{}",
            describe(&output)
        );
        output
    }
}

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh")
}

fn write_archive(archive: &Path, binary: &str) {
    let payload = tempfile::tempdir().unwrap();
    fs::write(payload.path().join("quality"), binary).unwrap();
    let status = Command::new("tar")
        .arg("-czf")
        .arg(archive)
        .arg("-C")
        .arg(payload.path())
        .arg("quality")
        .status()
        .unwrap();
    assert!(status.success());
}

fn available(programs: &[&'static str]) -> Vec<&'static str> {
    let mut available = Vec::new();
    for &program in programs {
        if which::which(program).is_ok() {
            available.push(program);
        } else {
            eprintln!("skipping {program} installer checks because it is not installed");
        }
    }
    assert!(!available.is_empty(), "none of {programs:?} is installed");
    available
}

fn assert_installed(release: &Release, output: &Output, case: &str) {
    let context = format!("{case}\n{}", describe(output));
    assert!(output.status.success(), "{context}");
    assert_eq!(
        release.installed_binary().as_deref(),
        Some(RELEASED_BINARY),
        "{context}"
    );
}

fn assert_rejected(release: &Release, output: &Output, error: &str, case: &str) {
    let context = format!("{case}\n{}", describe(output));
    assert_eq!(output.status.code(), Some(1), "{context}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(error),
        "{context}"
    );
    assert_eq!(release.installed_binary(), None, "{context}");
}

fn describe(output: &Output) -> String {
    format!(
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn entry_error(asset: &str) -> String {
    format!("The checksum file must contain exactly one SHA-256 entry for {asset}.")
}

fn mismatch_error(asset: &str) -> String {
    format!("Checksum mismatch for {asset}:")
}

fn current_supported_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        pair => panic!("installer test does not support {pair:?}"),
    }
}

fn file_digest(path: &Path, bits: &str) -> String {
    let gnu_utility = format!("sha{bits}sum");
    for (program, args) in [
        (gnu_utility.as_str(), Vec::new()),
        ("shasum", vec!["-a", bits]),
    ] {
        let output = Command::new(program).args(args).arg(path).output();
        if let Ok(output) = output {
            if output.status.success() {
                return String::from_utf8(output.stdout)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .to_owned();
            }
        }
    }
    panic!("no SHA-{bits} utility available")
}
