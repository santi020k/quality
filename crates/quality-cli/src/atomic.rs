use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

pub fn write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;

    let mut temporary = new_temp_file_in(parent)
        .with_context(|| format!("could not create a temporary file in {}", parent.display()))?;
    // Narrow the temp file's mode to match an existing, more restrictive
    // destination (e.g. 0600) *before* writing `contents` into it, so the
    // new contents never sit in a world/group-readable file even briefly.
    #[cfg(unix)]
    preserve_existing_permissions(&temporary, path)?;
    temporary
        .write_all(contents)
        .with_context(|| format!("could not write temporary file for {}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("could not sync temporary file for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not atomically replace {}", path.display()))?;
    Ok(())
}

// If `path` already exists, carry its exact mode (e.g. a deliberately
// narrowed 0600, or a shared 0640/0664) onto the replacement temp file so the
// atomic swap can't widen or narrow it. New files fall through untouched and
// keep the umask-respecting mode `new_temp_file_in` already requested, which
// matches what `File::create`/`fs::write` would have produced.
#[cfg(unix)]
fn preserve_existing_permissions(temporary: &NamedTempFile, path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) => temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .with_context(|| format!("could not preserve permissions for {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("could not read metadata for {}", path.display()))
        }
    }
}

// `NamedTempFile` defaults to mode 0600 on Unix regardless of umask, which is
// appropriate for secret-bearing scratch files but would silently narrow the
// permissions of every config, baseline, and report file this CLI replaces.
// Request the same 0o666 mode `File::create`/`fs::write` use so the kernel's
// umask handling produces the same result a plain write would have for files
// that don't already exist.
#[cfg(unix)]
fn new_temp_file_in(parent: &Path) -> std::io::Result<NamedTempFile> {
    use std::os::unix::fs::PermissionsExt;

    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o666))
        .tempfile_in(parent)
}

#[cfg(not(unix))]
fn new_temp_file_in(parent: &Path) -> std::io::Result<NamedTempFile> {
    NamedTempFile::new_in(parent)
}

#[cfg(unix)]
pub fn write_executable(path: &Path, contents: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    write_with_permissions(path, contents, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
pub fn write_executable(path: &Path, contents: &[u8]) -> Result<()> {
    write(path, contents)
}

#[cfg(unix)]
fn write_with_permissions(
    path: &Path,
    contents: &[u8],
    permissions: fs::Permissions,
) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("could not create a temporary file in {}", parent.display()))?;
    temporary
        .write_all(contents)
        .with_context(|| format!("could not write temporary file for {}", path.display()))?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .with_context(|| format!("could not set permissions for {}", path.display()))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("could not sync temporary file for {}", path.display()))?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not atomically replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomically_replaces_existing_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        fs::write(&path, b"old").unwrap();

        write(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn new_file_matches_plain_write_mode_under_current_umask() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();

        // A plain write establishes what this platform's umask would produce
        // for a brand-new file; atomic writes of a brand-new file should
        // match it rather than forcing the temp-file crate's
        // secret-scratch-file default (0600).
        let plain_path = directory.path().join("plain.json");
        fs::write(&plain_path, b"contents").unwrap();
        let expected_mode = fs::metadata(&plain_path).unwrap().permissions().mode() & 0o777;

        let atomic_path = directory.path().join("atomic.json");
        write(&atomic_path, b"contents").unwrap();
        let mode = fs::metadata(&atomic_path).unwrap().permissions().mode() & 0o777;

        assert_eq!(
            mode, expected_mode,
            "a new file written atomically should get the same mode a plain write would"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_existing_mode_across_replacement() {
        use std::os::unix::fs::PermissionsExt;

        for mode in [0o600, 0o640, 0o664, 0o755] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("report.json");
            fs::write(&path, b"old").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();

            write(&path, b"new").unwrap();

            assert_eq!(fs::read(&path).unwrap(), b"new");
            let actual_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                actual_mode, mode,
                "atomic write must preserve the destination's existing {mode:o} mode exactly"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn write_executable_remains_executable_for_new_and_existing_files() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();

        let new_path = directory.path().join("new-script.sh");
        write_executable(&new_path, b"#!/bin/sh\n").unwrap();
        assert_eq!(
            fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
            0o755
        );

        let existing_path = directory.path().join("existing-script.sh");
        fs::write(&existing_path, b"old").unwrap();
        fs::set_permissions(&existing_path, fs::Permissions::from_mode(0o600)).unwrap();

        write_executable(&existing_path, b"#!/bin/sh\n").unwrap();

        assert_eq!(fs::read(&existing_path).unwrap(), b"#!/bin/sh\n");
        assert_eq!(
            fs::metadata(&existing_path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_not_found_metadata_error_fails_without_partial_write() {
        // A self-referencing symlink makes `fs::metadata` fail with a loop
        // error (not `NotFound`), exercising the explicit non-NotFound error
        // path in `preserve_existing_permissions` without needing to touch
        // the parent directory's own permissions (which would instead just
        // fail temp-file creation).
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("looping-symlink");
        std::os::unix::fs::symlink(&path, &path).unwrap();

        let result = write(&path, b"new");

        assert!(result.is_err());
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "no stray temporary file should remain after a failed write"
        );
    }
}
