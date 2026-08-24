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

## `quality preset`

Inspect or apply built-in, language-aware analyzer policies:

```bash
quality preset list
quality preset show recommended
quality preset apply recommended --dry-run
quality preset apply recommended
quality preset apply strict --install
quality preset diff
quality preset update --dry-run
quality preset update --install
quality preset setup
quality preset setup --install
```

The profiles are `minimal`, `recommended`, and `strict`. Automatic detection
covers JavaScript/TypeScript/Astro, Python, Rust, Swift, Kotlin/Android, and GitHub
Actions. Use `--only javascript,rust` to limit generation, `--force` to replace
differing target files after review, and `--gate fast|full` to select an
existing root package script for the generated `quality.yml`.

Application performs a full conflict check before writing anything. Files with
the intended contents are left unchanged. Without `--install`, the CLI prints
the pinned package-manager command needed by JavaScript presets; with it, only
missing dependencies are installed.

Applying a preset writes `.quality-preset.json`, which records the preset
catalog version and generated-file fingerprints. `preset diff` exits with
status 1 when files, dependency pins, or the catalog differ. `preset update`
refreshes untouched generated files, merges `quality.yml`, and replaces only
the marked Kotlin block in `.editorconfig`; edited whole-file targets require
an explicit `--force`.

JavaScript setup adds the matching `@santi020k/eslint-config-*` packages for
detected frameworks. `preset setup` prints native installation or Gradle/SwiftPM
guidance, while `preset setup --install` executes supported platform commands.

## `quality doctor`

Validate configuration and explain enabled, available, optional, and missing
tools. For applied presets, doctor also reports whether the catalog and pinned
dependencies are current, need an update, or are incompatible.

## `quality check`

Run all applicable analyzers concurrently and normalize their diagnostics.

```bash
quality check --format github --report quality.sarif
quality check --changed origin/main
quality check --report-level warning --fail-level error
quality check --fail-fast
quality check --jobs 4 --timeout-seconds 120
quality check --max-output-bytes 1048576
quality check --require-checks
quality check --only eslint --only astro-check
quality check --exclude cargo-clippy
```

`--report-level` controls which diagnostics are displayed and written to SARIF. `--fail-level` independently controls which severities fail the command.

JSON output includes `schema_version: 1` and an aggregate `summary` with tool
states, severity counts, affected files, and counts by rule. The published
[`quality` report schema](/quality-report.schema.json) defines the complete
machine-readable contract.

Use repeatable `--only ID` or `--exclude ID` flags to select built-in adapters,
repository tasks, or custom tools. Comma-separated IDs are also accepted. The
selection is recorded in JSON and SARIF output so automated reports retain the
exact execution scope.

`--jobs` bounds concurrent analyzer processes and defaults to the machine's
available parallelism. `--timeout-seconds` overrides configured adapter
timeouts. Analyzer output is drained safely while only the first
`--max-output-bytes` bytes are retained; JSON reports mark truncated output.
Use `--require-checks` in CI to reject an empty policy. Changed-file mode may
still complete successfully with zero executed tools when configured checks
exist but none apply to the changed files.

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

## `quality hooks`

Install the Git hook launchers declared in `quality.yml`, verify their status,
or remove only launchers managed by quality:

```bash
quality hooks install
quality hooks status
quality hooks uninstall
```

Git calls `quality hooks run <event>` through the managed launchers. Hook steps
run in order, stop at the first failure, and can receive Git's hook arguments
with `pass_hook_args: true`.

## `quality ci github`

Generate a GitHub Actions workflow with an explicit installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.3.1 --locked'
```

The generator selects Linux or macOS from the detected platforms and adds
package-manager setup, frozen dependency installation, and detected native
toolchain setup before running `quality doctor`.

## `quality repositories`

Audit every immediate Git repository under a parent folder without changing it:

```bash
quality --root ~/Projects repositories audit
quality --root ~/Projects repositories audit --format json
quality --root ~/Projects repositories audit --fail-on invalid
quality --root ~/Projects repositories audit --fail-on missing-configuration,missing-toolchain
```

Audits are report-only and exit successfully by default, even when they find a
problem. Use the repeatable, comma-separated `--fail-on` option to make an audit
exit unsuccessfully when it finds `invalid`, `missing-configuration`, or
`missing-toolchain`. The selected exit policy does not change pretty or JSON
report contents.

Create `quality.yml` only in repositories that do not already have one. Existing
configuration is never replaced:

```bash
quality --root ~/Projects repositories apply --dry-run
quality --root ~/Projects repositories apply --format json
```

The adoption report includes readiness state, detected adapters, generated
tasks, the exact IDs of missing toolchains, invalid configurations, and created
files for every repository. Pretty output prints missing IDs below the affected
repository, while JSON exposes them through `missing_toolchains`.

All commands accept `--root PATH` when the target repository is not the current directory.

See [compatibility and support](/compatibility/) for stable-contract rules,
published JSON schemas, platform coverage, and the documented exit codes.
