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
| JavaScript/TypeScript | Biome | ✓ | format and fix |
| JavaScript/TypeScript | Oxlint | ✓ | fix |
| Astro | Astro Check | ✓ | — |
| JavaScript/TypeScript | Prettier | ✓ | format |
| Content | CSpell | ✓ | — |
| Content | Codespell | ✓ | fix |
| Content | Typos | ✓ | fix |
| JavaScript/TypeScript | Knip | ✓ | — |
| GitHub Actions | Actionlint | ✓ | — |
| Web metadata | `@santi020k/og` | ✓ | — |

Cargo workspaces, Astro applications, Swift packages, Xcode projects, and
Android Gradle wrappers are located independently. In a monorepo, each adapter
runs from the workspace it belongs to and diagnostics remain relative to the
repository root.

The `santi-og` adapter is enabled when a workspace declares
`@santi020k/og`. It runs `santi-og check --json`, converts every missing or
stale generated asset into a normalized diagnostic, and leaves regeneration to
the explicit `santi-og generate` command. In changed-file mode, any change in
that workspace triggers the package's fast, content-aware integrity check. Run
only this adapter with `quality check --only santi-og`; see the
[`@santi020k/og` package guide](https://og.santi020k.com/docs/) for generation,
caching, and built-site auditing.

## Choosing a JavaScript linter and formatter

ESLint, Prettier, Biome, and Oxlint are detected independently from
dependencies, package scripts, and configuration files such as `biome.json`,
`biome.jsonc`, `.oxlintrc.json`, or `oxlint.config.ts`. Each adapter prefers the
workspace's `node_modules/.bin` executable and reports through its native
GitHub annotation format.

- `biome` runs `biome check` for linting, formatting, and import sorting;
  `quality format` and `quality fix` use `biome format --write` and
  `biome check --write`. `quality format --check` runs `biome format` without
  linting; when `format_args` is customized, it reuses those arguments without
  `--write` or `--fix` and adds `--reporter=github` unless a reporter is set.
- `oxlint` runs `oxlint --format=github` and applies `oxlint --fix`. It has no
  formatter.

Choose the stack per repository with `tools.<id>.enabled`; detection is only
the default. Repositories managed by Pawl leave these engines to Pawl; see
[Pawl integration (beta)](#pawl-integration-beta). Common combinations are ESLint with Prettier, Biome alone, Oxlint
with Prettier, Oxlint with ESLint (Oxlint for fast rules and ESLint, for example
with `eslint-plugin-oxlint`, for the rest), and ESLint with Biome as the
formatter:

```yaml
version: 1
tools:
  oxlint:
    enabled: true
  eslint:
    enabled: true
  prettier:
    enabled: true
  biome:
    enabled: false
```

Biome's own configuration decides its role. Set `"formatter": { "enabled": false }`
in `biome.json` to use Biome only as a linter next to Prettier, or
`"linter": { "enabled": false }` to use it only as a formatter next to ESLint or
Oxlint. Do not enable two formatters for the same files: Prettier and the Biome
formatter would race during `quality format`, so disable one of them.

In changed-file mode, Biome receives changed JavaScript, TypeScript, JSON, CSS,
GraphQL, HTML, Astro, Vue, and Svelte files, and Oxlint receives changed
JavaScript, TypeScript, Vue, Svelte, and Astro files. Changes to their
configuration, `package.json`, lockfiles, `tsconfig`, or `quality.yml` re-run
the whole project. Use `quality check --changed` or `quality check --only oxlint`
for fast partial runs.

## Pawl integration (beta)

> **Beta — in progress.** [Pawl](https://pawl.santi020k.com) (`@santi020k/pawl`)
> compiles one rule standard for Biome, Oxlint, ESLint, and Prettier and adds
> a baseline ratchet so existing findings can only shrink. A dedicated `pawl`
> adapter is planned once Pawl's JSON report is stable; until then, the
> behavior below is the only integration.

Pawl runs the JavaScript engines itself, against its own baseline in
`.pawl/baseline.json`. Running them again from Quality would check the same
files twice and fail on findings Pawl intentionally allows. A repository is
treated as Pawl-managed when a `package.json` depends on `@santi020k/pawl`, a
package script runs `pawl`, a workspace package is named `@santi020k/pawl`, or a
`pawl.config.{ts,mts,js,mjs,json}` file exists. In that case `biome`, `oxlint`,
`eslint`, and `prettier` are no longer auto-detected, even when their own
configuration files are present.

The decision applies to the whole repository: in a monorepo, one workspace
that adopts Pawl turns off auto-detection of these four adapters for every
workspace, and `quality doctor` lists them as not detected. Enable the adapters
you still want Quality to run in `quality.yml`.

Explicit configuration always wins. Run Pawl as a repository task in the
meantime, and enable an engine adapter only when you want Quality to run it in
addition to Pawl:

```yaml
version: 1
tasks:
  pawl:
    name: Pawl
    command: pnpm
    args: [exec, pawl, check]
    extensions: [js, jsx, ts, tsx, mjs, cjs, mts, cts, json, css, astro, vue, svelte]
    config_files: [package.json, biome.json, .oxlintrc.json, pawl.config.ts, .pawl/baseline.json]
    required: true
```

The planned adapter will run `pawl check --format json` on changed files,
report each finding with its engine rule, apply `pawl fix` for `quality fix`,
and report Pawl setup problems (exit code 2) as toolchain failures rather than
code findings.

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

Supported parsers are `generic`, `codespell`, `eslint-json`,
`github-annotations`, `swiftlint-json`, `ktlint-json`, `santi-og-json`, and
`typos-json`. The `github-annotations` parser reads GitHub workflow-command
lines such as `::error file=src/app.ts,line=3,col=5,title=rule-id::Message`
from standard output. Generic diagnostics use:

```text
path/to/file:line:column: warning: Message (rule-id)
```

External adapters participate in diagnostics, changed-file filtering, concurrent execution, GitHub annotations, SARIF, and baselines.
