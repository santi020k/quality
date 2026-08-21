---
title: Getting started
description: Install quality and run the first repository check.
---

## Install from source

Until native releases are published, install the CLI from a local checkout:

```bash
cargo install --path crates/quality-cli
```

The release workflow also produces checksum-protected archives for Linux, Apple Silicon and Intel macOS, and Windows.

## Initialize a repository

Run these commands from the repository you want to check:

```bash
quality init
quality doctor
quality check
```

`quality init` detects relevant files and creates a minimal `quality.yml`. It does not enable analyzers for ecosystems that are absent.

`quality doctor` explains which tools are enabled, where each executable resolves, and which required dependencies are missing.

## Check only your changes

For a quick local loop:

```bash
quality check --changed
```

To compare a branch with its base:

```bash
quality check --changed origin/main
```

File-capable tools receive relevant paths. Project analyzers such as Android Lint continue to run at project scope when Android files change.

## Next steps

- Review [configuration](/configuration/) options.
- Add [GitHub Actions](/github-actions/) annotations and SARIF.
- Use a [baseline](/changed-files-and-baselines/) for an existing codebase.
