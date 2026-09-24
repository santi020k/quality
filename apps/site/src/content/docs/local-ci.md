---
title: Local CI and PR gates
description: Run reproducible pull-request checks before pushing and inspect the remaining GitHub-only boundary.
---

`quality` can run a repository's version-controlled Git hook as a timed local CI
gate. This catches deterministic failures before a push consumes a hosted
runner, while GitHub Actions remains authoritative for the exact pushed commit.

## Inspect coverage

```bash
quality ci plan
quality ci plan --strict
quality ci plan --hook pre-commit --format json
```

The default plan compares pull-request workflows with `hooks.pre-push` in
`quality.yml`. Each workflow step is classified as:

- **covered** when its normalized `run:` command and working directory exactly
  match a local hook step;
- **GitHub-only** when it uses an action, expression, or conditional that needs
  GitHub context;
- **uncovered** when it is a plain command that could run locally but is absent
  from the selected hook.

The comparison is deliberately exact. It does not claim that a broad wrapper
script covers a different workflow command, because doing so would turn an
inference into false parity. Declare the relationship explicitly when a broader
local wrapper genuinely includes the CI command:

```yaml
hooks:
  pre-push:
    steps:
      - name: Run full local validation
        command: pnpm
        args: [run, pre-push]
        covers:
          - pnpm run ci
```

`covers` affects the plan only; execution still runs the declared hook command.
Use `--strict` in adoption checks to reject uncovered commands.

## Run the gate

```bash
quality ci local
quality ci local --hook pre-commit
quality ci local --step 2
```

Steps run sequentially and stop after the first failure. The terminal report
shows each step's wall time, the total wall time, the exit code and a concise
tail of bounded failure output, followed by an exact focused rerun command.
`--step` accepts a one-based number from the plan or prior report.

Managed Git hooks use this same execution and reporting path. A typical policy
keeps `pre-commit` limited to staged-file checks and uses `pre-push` for affected
tests and builds:

```yaml
hooks:
  pre-commit:
    steps:
      - name: Check staged files
        command: pnpm
        args: [exec, lint-staged]
  pre-push:
    steps:
      - name: Run affected validation
        command: pnpm
        args: [run, pre-push]
```

When a root package already declares `pre-commit`, `precommit`, `pre-push`, or
`prepush`, `quality init` and language-aware presets import that script as the
corresponding hook. Existing hook configuration is preserved.

## Reports and privacy

The 20 most recent runs store metadata without command output under
`.git/quality/local-ci/`. The history stays outside the worktree and therefore
cannot be committed accidentally. A history write failure emits a warning but
does not change the gate result. Disable history with `--no-history`.

Write a complete versioned report explicitly when another local tool needs the
captured output:

```bash
quality ci local --format json --report reports/local-ci.json
```

Combined output retention defaults to 1 MiB per step and can be changed with
`--max-output-bytes`. Explicit reports may contain anything printed by a check;
do not commit or share them when a command may expose sensitive information.
The contracts are published as the [plan schema](/quality-ci-plan.schema.json)
and [run-report schema](/quality-local-ci.schema.json).

## What remains in GitHub

Local CI does not emulate hosted runner images or execute arbitrary `uses:`
actions. Secrets, GitHub permissions, service containers, CodeQL and dependency
review services, deployment, release publication, and platform-specific hosted
checks remain GitHub-only. Keep one protected GitHub check for the exact pushed
commit even after the local gate passes.
