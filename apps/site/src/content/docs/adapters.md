---
title: Built-in and custom adapters
description: Supported analyzers and the extension model for additional tools.
---

## Built-in adapters

| Ecosystem | Analyzer | Check | Format or fix |
| --- | --- | :---: | :---: |
| Rust | Cargo fmt | ✓ | format |
| Rust | Clippy | ✓ | — |
| Swift | SwiftLint | ✓ | fix |
| Swift | SwiftFormat | ✓ | format |
| Android | Android Lint | ✓ | — |
| Kotlin | detekt | ✓ | — |
| Kotlin | ktlint | ✓ | format |
| JavaScript/TypeScript | ESLint | ✓ | fix |
| Astro | Astro Check | ✓ | — |
| JavaScript/TypeScript | Prettier | ✓ | format |
| Content | CSpell | ✓ | — |
| Content | Codespell | ✓ | fix |
| Content | Typos | ✓ | fix |
| JavaScript/TypeScript | Knip | ✓ | — |
| GitHub Actions | Actionlint | ✓ | — |

Cargo workspaces, Astro applications, Swift packages, Xcode projects, and
Android Gradle wrappers are located independently. In a monorepo, each adapter
runs from the workspace it belongs to and diagnostics remain relative to the
repository root.

## External adapters

Add an analyzer without changing the `quality` binary:

```yaml
version: 1
output: pretty
tools: {}
custom:
  acme-lint:
    name: ACME Lint
    command: ./tools/acme-lint
    extensions: [swift, kt]
    config_files: [.acme-lint.yml]
    check_args: [scan]
    format_check_args: [format, --check]
    format_args: [format]
    fix_args: [fix]
    file_mode: append
    parser: generic
```

Use `file_mode: append` to append changed source paths to configured arguments. Use `project` for analyzers that must inspect the whole repository.

Supported parsers are `generic`, `codespell`, `eslint-json`, `swiftlint-json`,
`ktlint-json`, and `typos-json`. Generic diagnostics use:

```text
path/to/file:line:column: warning: Message (rule-id)
```

External adapters participate in diagnostics, changed-file filtering, concurrent execution, GitHub annotations, SARIF, and baselines.
