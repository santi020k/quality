---
title: Overview
description: Understand what quality does and where it fits in your development workflow.
---

`quality` is a fast command-line orchestrator for code-quality tools across Swift, Android/Kotlin, JavaScript, and TypeScript projects.

It keeps each ecosystem's native analyzer as the source of truth. Instead of replacing Cargo, Clippy, SwiftLint, Android Lint, detekt, ktlint, ESLint, Astro Check, Prettier, CSpell, Knip, or Actionlint, it gives them a shared workflow:

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

## Planned: native Git hooks

`quality` is planned to provide an optional, package-manager-independent Git
hook runner. A small managed hook will delegate to `quality`, while the
version-controlled behavior remains in `quality.yml`:

```yaml
hooks:
  pre-commit:
    steps:
      - name: Check staged code
        quality:
          operation: check
          scope: staged

      - name: Check generated files
        command: pnpm
        args: [run, check:generated]

  commit-msg:
    steps:
      - name: Validate commit message
        command: commitprompt
        args: [validate, --input]
        pass_hook_args: true

  pre-push:
    steps:
      - name: Check pushed changes
        quality:
          operation: check
          scope: branch

      - name: Run tests
        command: pnpm
        args: [test]
```

This design lets teams add repository-specific steps without editing generated
hook files or depending on Node.js. Steps run sequentially and stop at the
first failure by default. The planned lifecycle is `quality hooks install`,
`quality hooks status`, and `quality hooks uninstall`; changing steps will not
require reinstalling the managed hooks.

Installation will be explicit and conflict-safe. `quality` will not overwrite
hooks owned by Husky, Lefthook, or another manager. When an existing manager is
detected, it will instead provide the appropriate `quality hooks run <event>`
integration command. Local hooks remain an early feedback mechanism; protected
CI checks remain the authoritative quality gate.

## Supported output

- `pretty` for local development
- `json` for automation
- `sarif` for code-scanning platforms
- `github` for native workflow annotations

Use `--report quality.sarif` to write a SARIF artifact while retaining readable terminal output.
