---
title: GitHub Actions
description: Add pull-request annotations, SARIF, and changed-file checks.
---

Generate a starter workflow with an explicit, pinned installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.1.0 --locked'
```

The generated workflow:

1. Checks out complete Git history for change detection.
2. Installs `quality` using the command you supplied.
3. Runs `quality doctor` to expose environment problems.
4. Checks pull-request changes against the base branch.
5. Emits native GitHub annotations.
6. Uploads a SARIF report to code scanning.

## Cost-aware CI

Run JavaScript, Android, Kotlin, and Rust jobs on Linux whenever platform requirements permit. Reserve macOS runners for Swift and Xcode work.

Cancel obsolete pull-request runs when a newer commit arrives:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

For monorepos, combine `quality check --changed` with Turborepo's affected-package selection so unchanged applications never start expensive jobs.
