---
title: Getting started
description: Install quality and run the first repository check.
---

## Install a native release

On macOS or Linux:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/santi020k/quality/main/install.sh \
  | sh -s -- santi020k/quality v0.1.0
```

Install from a local checkout when developing the CLI itself:

```bash
cargo install --path crates/quality-cli
```

The release includes checksum-protected archives for x86-64 and ARM64 Linux,
Apple Silicon and Intel macOS, and x86-64 Windows.

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
