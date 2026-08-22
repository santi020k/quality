---
title: Commands
description: Command reference for the quality CLI.
---

## `quality init`

Detect repository ecosystems and write a starter `quality.yml`. Existing configuration is not overwritten implicitly.

## `quality doctor`

Validate configuration and explain enabled, available, optional, and missing tools.

## `quality check`

Run all applicable analyzers concurrently and normalize their diagnostics.

```bash
quality check --format github --report quality.sarif
quality check --changed origin/main
quality check --report-level warning --fail-level error
quality check --fail-fast
```

`--report-level` controls which diagnostics are displayed and written to SARIF. `--fail-level` independently controls which severities fail the command.

## `quality format`

Apply configured formatters:

```bash
quality format
```

Verify formatting without changing files:

```bash
quality format --check
```

## `quality fix`

Apply safe fixes exposed by configured analyzers:

```bash
quality fix --changed
```

## `quality baseline create`

Record current, fully parsed findings so adoption can focus on new regressions:

```bash
quality baseline create
quality baseline create --force
```

## `quality completions`

Generate completions for Bash, Zsh, Fish, PowerShell, or Elvish:

```bash
quality completions zsh
```

## `quality ci github`

Generate a GitHub Actions workflow with an explicit installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.1.0 --locked'
```

All commands accept `--root PATH` when the target repository is not the current directory.
