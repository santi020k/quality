---
title: Commands
description: Command reference for the quality CLI.
---

## `quality init`

Detect intentionally configured analyzers and write a starter `quality.yml`.
Existing configuration is not overwritten implicitly. A root
`verify:quality`, `verify`, `validate`, `check`, `pre-push`, or `prepush`
package script is preserved as the canonical `repository-check` task without
duplicating analyzer checks.
Otherwise, a root `typecheck` or `type-check` script becomes a change-aware
task.

Use `quality init --dry-run` to print the generated policy without creating or
replacing `quality.yml`.

Choose the canonical repository gate explicitly when a project exposes both a
fast local workflow and a complete release workflow:

```bash
quality init --gate fast
quality init --gate full
```

## `quality doctor`

Validate configuration and explain enabled, available, optional, and missing tools.

## `quality check`

Run all applicable analyzers concurrently and normalize their diagnostics.

```bash
quality check --format github --report quality.sarif
quality check --changed origin/main
quality check --report-level warning --fail-level error
quality check --fail-fast
quality check --only eslint --only astro-check
quality check --exclude cargo-clippy
```

`--report-level` controls which diagnostics are displayed and written to SARIF. `--fail-level` independently controls which severities fail the command.

JSON output includes an aggregate `summary` with tool states, severity counts,
affected files, and counts by rule.

Use repeatable `--only ID` or `--exclude ID` flags to select built-in adapters,
repository tasks, or custom tools. Comma-separated IDs are also accepted. The
selection is recorded in JSON and SARIF output so automated reports retain the
exact execution scope.

## `quality format`

Apply configured formatters:

```bash
quality format
```

Verify formatting without changing files:

```bash
quality format --check
quality format --only prettier
```

## `quality fix`

Apply safe fixes exposed by configured analyzers:

```bash
quality fix --changed
quality fix --exclude swiftformat
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

## `quality instructions`

Print deterministic instructions for an AI coding agent without modifying the
repository:

```bash
quality instructions --format agents
```

Paste the output into the consuming repository's `AGENTS.md`. See
[AI coding agents](/ai-agents/) for the complete workflow.

## `quality ci github`

Generate a GitHub Actions workflow with an explicit installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.3.0 --locked'
```

The generator selects Linux or macOS from the detected platforms and adds
package-manager setup, frozen dependency installation, and detected native
toolchain setup before running `quality doctor`.

## `quality repositories`

Audit every immediate Git repository under a parent folder without changing it:

```bash
quality --root ~/Projects repositories audit
quality --root ~/Projects repositories audit --format json
```

Create `quality.yml` only in repositories that do not already have one. Existing
configuration is never replaced:

```bash
quality --root ~/Projects repositories apply --dry-run
quality --root ~/Projects repositories apply --format json
```

The JSON adoption report includes readiness state, detected adapters, generated
tasks, missing toolchains, invalid configurations, and created files for every
repository.

All commands accept `--root PATH` when the target repository is not the current directory.
