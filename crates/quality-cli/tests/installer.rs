#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn installer_downloads_verifies_and_installs_a_release_archive() {
    let target = current_supported_target();
    let temp = tempfile::tempdir().unwrap();
    let repository = "acme/quality";
    let version = "v0.1.0";
    let release = temp
        .path()
        .join(repository)
        .join("releases/download")
        .join(version);
    let payload = temp.path().join("payload");
    let install = temp.path().join("bin");
    fs::create_dir_all(&release).unwrap();
    fs::create_dir(&payload).unwrap();
    fs::write(payload.join("quality"), "test release binary\n").unwrap();

    let asset = format!("quality-{target}.tar.gz");
    let archive = release.join(&asset);
    assert!(
        Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(&payload)
            .arg("quality")
            .status()
            .unwrap()
            .success()
    );
    let digest = sha256(&archive);
    fs::write(
        release.join(format!("{asset}.sha256")),
        format!("{digest}  {asset}\n"),
    )
    .unwrap();

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../install.sh");
    let output = Command::new("sh")
        .arg(script)
        .args([repository, version])
        .env(
            "QUALITY_RELEASE_BASE_URL",
            format!("file://{}", temp.path().display()),
        )
        .env("QUALITY_INSTALL_DIR", &install)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(install.join("quality")).unwrap(),
        "test release binary\n"
    );
}

fn current_supported_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        pair => panic!("installer test does not support {pair:?}"),
    }
}

fn sha256(path: &Path) -> String {
    for (program, args) in [("sha256sum", Vec::new()), ("shasum", vec!["-a", "256"])] {
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
    panic!("no SHA-256 utility available")
}
