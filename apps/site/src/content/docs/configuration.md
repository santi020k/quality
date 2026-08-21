---
title: Configuration
description: Configure built-in and external quality analyzers.
---

Configuration lives in `quality.yml` at the repository root.

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
- `required`: fail if its executable is unavailable
- `command`: override executable resolution
- `check_args`: replace arguments for checks
- `format_args`: replace formatting arguments
- `fix_args`: replace fix arguments

Set `required: false` when a tool is useful locally but should not make every environment fail.

## Repository-local commands

Relative commands resolve from the repository root:

```yaml
tools:
  detekt:
    enabled: true
    required: true
    command: ./gradlew
    check_args: [detekt]
```

JavaScript adapters automatically prefer executables under `node_modules/.bin`. Android Lint automatically prefers `./gradlew`.

## Validation

Unknown keys are rejected instead of silently ignored. Common typos include a suggestion so configuration mistakes fail early and clearly.
