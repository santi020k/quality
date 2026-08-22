---
title: Getting started
description: Install quality and run the first repository check.
---

## Install a native release

On macOS or Linux:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/santi020k/quality/main/install.sh \
  | sh -s -- santi020k/quality v0.2.1
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

Preview the generated policy without writing a file with `quality init
--dry-run`.

`quality init` creates a minimal `quality.yml`. ESLint, Prettier, SwiftLint,
and SwiftFormat require evidence that the repository intends to use them, such
as configuration, a dependency, or a package script; language files alone do
not enable them.

If the root package already exposes `verify:quality`, `verify`, `validate`,
`check`, `pre-push`, or `prepush`, initialization preserves that canonical gate as a repository task.
Detected analyzers remain available for formatting and fixes without repeating
their checks alongside the canonical script.

When there is no composite gate, a root `typecheck` or `type-check` script is
imported as a change-aware task so existing workspace and Turborepo behavior is
preserved.

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
