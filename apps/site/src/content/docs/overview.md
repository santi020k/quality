---
title: Overview
description: Understand what quality does and where it fits in your development workflow.
---

`quality` is a fast command-line orchestrator for code-quality tools across Swift, Android/Kotlin, JavaScript, and TypeScript projects.

It keeps each ecosystem's native analyzer as the source of truth. Instead of replacing Cargo, Clippy, SwiftLint, Android Lint, detekt, ktlint, ESLint, Astro Check, or Prettier, it gives them a shared workflow:

1. Detect the ecosystems present in a repository.
2. Resolve repository-local tools before global installations.
3. Run independent analyzers concurrently.
4. Normalize findings, failures, and exit behavior.
5. Report results for terminals, automation, and code review.

## Why use it?

Mixed repositories often accumulate several scripts, output formats, installation conventions, and CI steps. Developers must remember which command applies to which directory, while reviewers receive inconsistent feedback.

With `quality`, the common workflow is stable:

```bash
quality doctor
quality check
quality format --check
```

## Deterministic by design

The checking path is intentionally deterministic. Future AI integrations can consume normalized diagnostics to explain findings or propose changes, but analyzer execution and pass/fail behavior do not depend on an AI service.

## Supported output

- `pretty` for local development
- `json` for automation
- `sarif` for code-scanning platforms
- `github` for native workflow annotations

Use `--report quality.sarif` to write a SARIF artifact while retaining readable terminal output.
