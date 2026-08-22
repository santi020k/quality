---
title: Configuration
description: Configure built-in and external quality analyzers.
---

Configuration lives in `quality.yml` at the repository root.

`quality init` adds a YAML language-server comment that points compatible
editors to the published [`quality.yml` JSON Schema](/quality.schema.json).

```yaml
version: 1
output: pretty
baseline: .quality-baseline.json
tools:
  swiftlint:
    enabled: true
    required: true
  eslint:
    enabled: true
    required: true
```

## Tool settings

Each built-in adapter accepts:

- `enabled`: include or exclude the tool
- `check`: participate in `quality check`; set false to retain format and fix operations
- `required`: fail if its executable is unavailable
- `command`: override executable resolution
- `working_directory`: run from a repository-relative workspace directory
- `check_args`: replace arguments for checks
- `format_args`: replace formatting arguments
- `fix_args`: replace fix arguments
- `timeout_seconds`: stop an invocation that exceeds this duration

Set `required: false` when a tool is useful locally but should not make every environment fail.

## Repository-local commands

Relative commands resolve from the adapter's working directory:

```yaml
tools:
  detekt:
    enabled: true
    required: true
    working_directory: apps/android
    command: ./gradlew
    check_args: [detekt]
```

JavaScript adapters automatically prefer executables under `node_modules/.bin`. Android Lint locates nested Gradle wrappers automatically.

## Repository tasks

Use `tasks` for canonical project gates that should participate in `quality
check` without pretending to be a file-oriented formatter:

```yaml
tasks:
  typecheck:
    name: TypeScript
    command: pnpm
    args: [run, typecheck]
    extensions: [ts, tsx, astro]
    config_files: [package.json, tsconfig.json, pnpm-lock.yaml]
```

Tasks run concurrently with built-in and custom adapters. Set
`working_directory` for a workspace-specific command. When `extensions` or
`config_files` are present, changed-file mode skips the task unless one of
those inputs changed. With neither field, the task always runs.
Tasks and custom adapters also accept `timeout_seconds`.

During initialization, a canonical root package script is added as
`repository-check` when available. Detected analyzers receive `check: false`
so that script remains the source of truth without sacrificing `quality
format` or `quality fix`. If no composite script exists, `typecheck` or
`type-check` is imported separately and analyzers continue to check normally.

## Validation

Unknown keys are rejected instead of silently ignored. Common typos include a suggestion so configuration mistakes fail early and clearly.
