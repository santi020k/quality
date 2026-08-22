---
title: GitHub Actions
description: Run changed-file checks with annotations, summaries, and SARIF.
---

The official Action installs a verified `quality` release, checks pull-request changes, and writes native annotations and a job summary:

```yaml
name: Quality

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read
  security-events: write

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  quality:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: santi020k/quality@v0.2.1
        id: quality
        with:
          version: v0.2.1
          changed-only: true
          report-level: warning
          fail-level: warning

      - name: Upload code-scanning results
        if: always() && steps.quality.outputs.sarif != ''
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: ${{ steps.quality.outputs.sarif }}
```

Pin `version` to a release for reproducible checks. Warnings fail by default; set `fail-level: error` only for an intentional, documented migration period.

The Action exposes `sarif`, `findings`, `tools`, and `duration-ms` outputs. On pull requests it compares against `origin/$GITHUB_BASE_REF`; on pushes without a base it safely checks the complete project.

## Generate a standalone workflow

Generate a workflow with an explicit installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.2.1 --locked'
```

The generated workflow:

1. Checks out complete Git history for change detection.
2. Selects macOS for Swift repositories and Linux otherwise.
3. Sets up the detected package manager and installs frozen dependencies.
4. Sets up detected Rust, Java/Gradle, SwiftLint, SwiftFormat, and Actionlint requirements.
5. Installs `quality` using the command you supplied.
6. Runs `quality doctor` to expose environment problems.
7. Checks pull-request changes against the base branch.
8. Emits native GitHub annotations and uploads SARIF to code scanning.

## Cost-aware CI

Run JavaScript, Android, Kotlin, and Rust jobs on Linux whenever platform requirements permit. Reserve macOS runners for Swift and Xcode work.

Cancel obsolete pull-request runs when a newer commit arrives:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

For monorepos, combine the Action's `changed-only` mode with Turborepo's affected-package selection so unchanged applications never start expensive jobs.

## Reporting and failure levels

Reporting and build policy are independent:

```bash
quality check --report-level warning --fail-level error
```

Warnings appear as annotations and in SARIF, while only errors fail the job. Required tools that are missing always fail regardless of severity settings.
