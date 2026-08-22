---
title: Overview
description: Understand what quality does and where it fits in your development workflow.
---

`quality` is a fast command-line orchestrator for code-quality tools across Swift, Android/Kotlin, Python, JavaScript, and TypeScript projects.

It keeps each ecosystem's native analyzer as the source of truth. Instead of replacing Cargo, Clippy, SwiftLint, Android Lint, detekt, ktlint, ESLint, Astro Check, Prettier, CSpell, Codespell, Typos, Knip, or Actionlint, it gives them a shared workflow:

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

## Native Git hooks

`quality` provides an optional, package-manager-independent Git hook runner. A
small managed hook delegates to `quality`, while the version-controlled behavior
remains in `quality.yml`:

```yaml
hooks:
  pre-commit:
    steps:
      - name: Check staged code
        command: pnpm
        args: [exec, lint-staged]

  commit-msg:
    steps:
      - name: Validate commit message
        command: commitprompt
        args: [validate, --input]
        pass_hook_args: true

  pre-push:
    steps:
      - name: Run repository checks
        command: pnpm
        args: [run, validate]
```

This design lets teams add repository-specific steps without editing generated
hook files or depending on Node.js. Steps run sequentially and stop at the
first failure. The lifecycle is `quality hooks install`,
`quality hooks status`, and `quality hooks uninstall`; changing steps will not
require reinstalling the managed hooks.

When `@santi020k/commitprompt` is already installed, `quality init` and the
language-aware presets add this `commit-msg` policy automatically. An existing
`commit-msg` configuration is always preserved.

Installation is explicit and conflict-safe. `quality` does not overwrite
hooks owned by Husky, Lefthook, or another manager. Remove the previous
manager's `core.hooksPath` setting before installing, or invoke
`quality hooks run <event>` from that manager manually. Local hooks remain an
early feedback mechanism; protected CI checks remain the authoritative quality
gate.

## Supported output

- `pretty` for local development
- `json` for automation
- `sarif` for code-scanning platforms
- `github` for native workflow annotations

Use `--report quality.sarif` to write a SARIF artifact while retaining readable terminal output.
