use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::{DirEntry, WalkDir};

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    file_names: BTreeSet<String>,
    extensions: BTreeSet<String>,
    relative_paths: BTreeSet<PathBuf>,
}

impl Project {
    pub fn discover(root: &Path) -> Result<Self> {
        let mut file_names = BTreeSet::new();
        let mut extensions = BTreeSet::new();
        let mut relative_paths = BTreeSet::new();

        for entry in WalkDir::new(root)
            .max_depth(6)
            .into_iter()
            .filter_entry(should_visit)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                file_names.insert(name.to_owned());
            }
            if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
                extensions.insert(extension.to_ascii_lowercase());
            }
            if let Ok(relative) = path.strip_prefix(root) {
                relative_paths.insert(relative.to_owned());
            }
        }

        Ok(Self {
            root: root.to_owned(),
            file_names,
            extensions,
            relative_paths,
        })
    }

    pub fn has_file(&self, name: &str) -> bool {
        self.file_names.contains(name)
    }

    pub fn has_extension(&self, extension: &str) -> bool {
        self.extensions.contains(extension)
    }

    pub fn path_contains(&self, fragment: &str) -> bool {
        self.relative_paths
            .iter()
            .any(|path| path.to_string_lossy().contains(fragment))
    }
}

fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | ".build" | "build" | "DerivedData" | "node_modules" | "target" | "vendor")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_build_directories() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("App.swift"), "").unwrap();
        std::fs::create_dir(temp.path().join("build")).unwrap();
        std::fs::write(temp.path().join("build/Generated.kt"), "").unwrap();
        let project = Project::discover(temp.path()).unwrap();
        assert!(project.has_extension("swift"));
        assert!(!project.has_extension("kt"));
    }
}
