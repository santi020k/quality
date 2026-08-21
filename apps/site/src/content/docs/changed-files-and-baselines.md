---
title: Changed files and baselines
description: Keep local and pull-request feedback focused without hiding failures.
---

## Changed-file mode

Use `--changed` with checks, formatting, or fixes:

```bash
quality check --changed
quality check --changed origin/main
quality format --changed
quality fix --changed
```

Without a base, `quality` includes staged, unstaged, and untracked files. With a base, it also includes branch changes since that revision.

Configuration changes trigger the corresponding full analyzer because a rules change can affect files that were not edited. Project-wide analyzers also retain their required project scope.

## Baseline an existing repository

Create a baseline after verifying every required tool is installed and runs successfully:

```bash
quality baseline create
git add .quality-baseline.json quality.yml
```

Existing matching findings are suppressed on later checks; new findings still fail.

Fingerprints exclude line and column positions, so moving code does not create noise. Duplicate occurrences are counted, which means adding another copy of an existing violation is still detected.

## Safety rules

A baseline is refused when a required tool is missing, crashes, or returns output that cannot be fully parsed. Infrastructure failures must never become accepted code-quality debt.
