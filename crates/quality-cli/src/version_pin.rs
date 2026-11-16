use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;

const VERSION_FILE: &str = ".quality-version";

pub fn enforce(root: &Path) -> Result<()> {
    check(root, env!("CARGO_PKG_VERSION"))
}

fn check(root: &Path, actual: &str) -> Result<()> {
    let path = root.join(VERSION_FILE);
    if !path.is_file() {
        return Ok(());
    }
    let required =
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    let required = required.trim();
    if required.is_empty() {
        anyhow::bail!("{VERSION_FILE} is empty; write a version such as `1.2.0`");
    }
    if !version_pattern().is_match(required) {
        anyhow::bail!(
            "invalid version `{required}` in {VERSION_FILE}; use `MAJOR.MINOR.PATCH` with optional prerelease or build metadata"
        );
    }
    if required != actual {
        anyhow::bail!(
            "this project requires quality {required} from {VERSION_FILE}, but quality {actual} is running\n{}",
            install_guidance(required)
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_guidance(required: &str) -> String {
    format!(
        "Install the required checksum-verified release:\n\
         curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/santi020k/quality/v{required}/install.sh | sh -s -- santi020k/quality v{required}"
    )
}

#[cfg(windows)]
fn install_guidance(required: &str) -> String {
    format!(
        "Download the Windows archive and its SHA-256 file from the quality v{required} release, verify the checksum, and replace the active quality.exe:\n\
         https://github.com/santi020k/quality/releases/tag/v{required}"
    )
}

fn version_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?(?:\+[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$",
        )
            .expect("valid quality version regular expression")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_matching_version_and_an_absent_pin() {
        let temporary = tempfile::tempdir().unwrap();
        check(temporary.path(), "1.2.0").unwrap();
        fs::write(temporary.path().join(VERSION_FILE), "1.2.0\n").unwrap();
        check(temporary.path(), "1.2.0").unwrap();
    }

    #[test]
    fn rejects_mismatched_and_invalid_versions_with_recovery_guidance() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join(VERSION_FILE), "1.3.0\n").unwrap();
        let mismatch = check(temporary.path(), "1.2.0").unwrap_err().to_string();
        assert!(mismatch.contains("requires quality 1.3.0"));
        assert!(mismatch.contains("v1.3.0"));

        fs::write(temporary.path().join(VERSION_FILE), "v1.3.0\n").unwrap();
        assert!(check(temporary.path(), "1.2.0").is_err());

        fs::write(
            temporary.path().join(VERSION_FILE),
            "1.3.0-beta.1+build.4\n",
        )
        .unwrap();
        check(temporary.path(), "1.3.0-beta.1+build.4").unwrap();
    }
}
