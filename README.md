# quality

`quality` is one fast, predictable code-quality workflow for repositories that
contain Rust, Swift, Android/Kotlin, Python, JavaScript, and Astro projects. It does not replace the
ecosystem's best analyzers. It detects, runs, and explains them through one CLI.

Website and documentation: <https://quality.santi020k.com>

[Documentation](https://quality.santi020k.com) ·
[GitHub Action](#github-action) ·
[Compatibility](https://quality.santi020k.com/compatibility/) ·
[Releases](https://github.com/santi020k/quality/releases) ·
[Changelog](CHANGELOG.md) ·
[Issues](https://github.com/santi020k/quality/issues) ·
[Contributing](CONTRIBUTING.md)

[![CI](https://github.com/santi020k/quality/actions/workflows/ci.yml/badge.svg)](https://github.com/santi020k/quality/actions/workflows/ci.yml)
[![CodeQL](https://github.com/santi020k/quality/actions/workflows/codeql.yml/badge.svg)](https://github.com/santi020k/quality/actions/workflows/codeql.yml)
[![GitHub release](https://img.shields.io/github/v/release/santi020k/quality)](https://github.com/santi020k/quality/releases/latest)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> Status: `0.4.x` preview. The configuration format may change before the first
> stable release.

## GitHub Action

```yaml
- uses: actions/checkout@v6
  with:
    fetch-depth: 0

- uses: santi020k/quality@v0.4.0
  with:
    version: v0.4.0
    changed-only: true
    report-level: warning
    fail-level: warning
```

The Action verifies the downloaded release checksum, adds pull-request annotations, writes a job summary, and produces SARIF for GitHub code scanning.

## The developer experience

```console
$ quality init
Created quality.yml
Next: quality doctor && quality check

$ quality doctor
Project: /work/mobile-app
Config:  quality.yml

  ✓ SwiftLint      swiftlint
  ✓ SwiftFormat    swiftformat
  ✓ Android Lint   /work/mobile-app/gradlew
  ✓ detekt         detekt
  ✓ ktlint         ktlint

$ quality check
  ✓ SwiftLint      0.42s
  ✓ SwiftFormat    0.18s
  ✓ Android Lint   5.31s
  ✓ detekt         1.24s
  ✓ ktlint         0.39s

Quality checks passed (5 tools).
```

Checks run concurrently by default. Use `--fail-fast` when a quick first
failure is more useful than the complete report.

Bound resource use with `--jobs`, configure per-adapter `timeout_seconds`, or
override all timeouts with `--timeout-seconds`. Each analyzer retains at most
1 MiB of combined output by default; change it with `--max-output-bytes`.
CI can use `--require-checks` to prevent an empty policy from passing while
changed-file runs may still skip every configured adapter when no input applies.

## Install for development

```bash
cargo install --path crates/quality-cli
```

Unix users can install a checksum-verified native binary without Rust:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/santi020k/quality/main/install.sh \
  | sh -s -- santi020k/quality v0.4.0
```

Omit the version to install the latest release. Native archives are published for
Intel and Apple Silicon macOS, x86-64 and ARM64 Linux, and x86-64 Windows.

Then run these commands from any repository:

```bash
quality init            # Write an explicit policy based on detected files
quality init --dry-run  # Preview adoption without writing quality.yml
quality init --gate fast # Prefer the repository's fast local gate
quality init --gate full # Prefer the repository's complete gate
quality preset list      # Compare built-in setup profiles
quality preset apply recommended --dry-run # Preview language configs and dependencies
quality preset apply recommended # Generate configs without overwriting existing files
quality preset apply strict --install # Generate strict configs and install pinned JS tools
quality doctor          # Explain what is enabled, installed, or missing
quality check           # Run applicable linters concurrently
quality --root ~/Projects repositories audit # Audit a folder of repositories
quality --root ~/Projects repositories audit --fail-on invalid,missing-configuration # Enforce audit findings in CI
quality --root ~/Projects repositories apply # Configure missing repositories
quality format          # Run applicable formatters
quality format --check  # Check formatting without modifying files
quality fix             # Apply fixes supported by the configured tools
quality baseline create # Record existing findings and block new regressions
quality completions zsh # Generate native shell completions
quality instructions --format agents # Print a section for a repository AGENTS.md
quality ci github --install '…' # Generate a runnable GitHub Actions workflow
quality hooks install   # Install the Git hooks declared in quality.yml
quality hooks status    # Verify that every configured hook is installed
```

For quick local feedback, scope checks, formatting, or fixes to Git changes:

```bash
quality check --changed                 # Staged, unstaged, and untracked files
quality check --changed origin/main     # Branch changes plus local changes
quality format --changed
quality fix --changed
```

File-capable tools receive only relevant changed files. Project analyzers such
as Android Lint still run at project scope when Android files change. Changing
a rules or configuration file—including deleting one—triggers the corresponding
full check. Deleted source paths can trigger project-wide checks but are never
passed to file-scoped tools.

Select adapters by ID for focused local or CI runs. Flags can be repeated or
receive comma-separated IDs, and work with `check`, `format`, and `fix`:

```bash
quality check --only eslint,astro-check
quality check --exclude cargo-clippy
quality fix --changed --only eslint
```

Selection details are retained in JSON and SARIF reports.

Every command accepts `--root PATH`. Check results support `pretty`, `json`,
`sarif`, and `github` output. The GitHub format emits native workflow commands
that become inline annotations on pull requests:

```bash
quality check --format github
```

Write SARIF while keeping readable terminal or GitHub output with `--report`:

```bash
quality check --report quality.sarif
quality check --format github --report artifacts/quality.sarif
```

## Supported adapters

| Ecosystem | Analyzer | Check | Format/fix |
| --- | --- | ---: | ---: |
| Rust | Cargo fmt | yes | yes |
| Rust | Clippy | yes | — |
| Swift | SwiftLint | yes | fix |
| Swift | SwiftFormat | yes | yes |
| Android | Android Lint | yes | — |
| Kotlin | detekt | yes | — |
| Kotlin | ktlint | yes | yes |
| JavaScript/TypeScript | ESLint | yes | fix |
| Astro | Astro Check | yes | — |
| JavaScript/TypeScript | Prettier | yes | yes |
| Content | CSpell | yes | — |
| Content | Codespell | yes | fix |
| Content | Typos | yes | fix |
| JavaScript/TypeScript | Knip | yes | — |
| GitHub Actions | Actionlint | yes | — |
| Web metadata | `@santi020k/og` | yes | — |

## Language-aware presets

Presets can bootstrap the analyzer configuration that `quality` runs. They are
explicit generators: the resulting files and pinned dependency command remain
visible in the repository, and no preset logic participates in the checking
path.

```bash
quality preset list
quality preset show recommended
quality preset apply recommended --dry-run
quality preset apply recommended
quality preset diff
quality preset update --dry-run
quality preset setup
```

`minimal` installs the essential ecosystem checks, `recommended` adds balanced
formatting, spelling, and unused-code policy, and `strict` tightens thresholds
and promotes warnings where the underlying analyzer supports it. JavaScript
presets use `@santi020k/eslint-config-basic`: minimal selects its `basic`
preset, recommended uses its recommended severity mode, and strict selects
`pedantic` mode.

Generation supports JavaScript/TypeScript/Astro, Python, Rust, Swift,
Kotlin/Android, and GitHub Actions. Limit an application with `--only`, preview
all proposed contents with `--dry-run`, or explicitly replace differing
generated targets with `--force`:

```bash
quality preset apply strict --only rust,github-actions
quality preset apply minimal --only javascript
```

JavaScript dependencies are pinned in the displayed install command. Pass
`--install` to run that command with the package manager declared by the root
project. Explicit framework packs are selected for detected Angular, Astro,
Expo, Hono, Lit, Nest, Next, Nuxt, Preact, Qwik, React, React Router, Slidev,
Solid, Svelte, TanStack Start, Vite, and Vue projects.

Each application records its catalog version, profile, ecosystems, managed
file fingerprints, and dependency pins in `.quality-preset.json`. Use
`quality preset diff` to detect catalog updates, dependency drift, or edited
files, then `quality preset update --dry-run` and `quality preset update` to
refresh untouched generated output. `quality doctor` reports current,
update-available, and incompatible preset states.

Preset updates merge tool policy into `quality.yml` without removing existing
tasks, hooks, or custom adapters. Kotlin rules use a marked managed block in
`.editorconfig`, leaving unrelated editor settings intact. Whole generated
files are replaced only while their recorded fingerprint proves they were not
edited, unless `--force` is explicitly supplied.

Run `quality preset setup` for platform-aware Python, Rust, Swift, Kotlin,
Android, spelling, and Actionlint setup guidance. Add `--install` to execute
supported commands.

Presets select one spelling adapter: CSpell for JavaScript repositories,
Codespell for Python repositories, and Typos for other recommended or strict
native-language repositories. Explicit `quality.yml` configuration can enable
a different combination.

The adapters use repository-local executables where that is conventional:
`./gradlew` for Android and `node_modules/.bin` for JavaScript. Other tools are
resolved from `PATH` or can be overridden in `quality.yml`.

## External adapters

Add an organization-specific or emerging analyzer without changing `quality`:

```yaml
version: 1
output: pretty
baseline: .quality-baseline.json
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

With `file_mode: append`, changed source paths are appended to the configured
arguments. Use `project` for analyzers that must always inspect the whole
project. Supported parsers are `generic`, `codespell`, `eslint-json`,
`swiftlint-json`, `ktlint-json`, `santi-og-json`, and `typos-json`. Generic
diagnostics use the familiar format:

```text
path/to/file:line:column: warning: Message (rule-id)
```

External adapters participate in `doctor`, concurrent execution, changed-file
filtering, JSON/SARIF reports, GitHub annotations, and baselines. Commands are
executed directly without a shell, so arguments remain explicit and portable.

When a JavaScript workspace declares `@santi020k/og`, the built-in `santi-og`
adapter runs its deterministic `check --json` command. Missing or stale social
images become normalized diagnostics; generation remains an explicit
`santi-og generate` operation that never runs as part of `quality check`.
Run only this adapter with:

```bash
quality check --only santi-og
```

See the [`@santi020k/og` package guide](https://og.santi020k.com/docs/) for
generation, caching, and built-site auditing.

## Configuration

`quality init` enables a tool only when the repository shows intent to use it,
such as an analyzer configuration, dependency, or package script. Merely
containing JavaScript or Swift files does not opt a repository into ESLint,
Prettier, SwiftLint, or SwiftFormat.

Use `quality init --dry-run` to review the complete generated policy before
writing or replacing `quality.yml`.

When the root package defines `verify:quality`, `verify`, `validate`, `check`,
`pre-push`, or `prepush`, initialization preserves the first matching script as a
`repository-check` task. Direct analyzer checks are disabled to avoid running
the same work twice, while their format and fix operations remain available:

```yaml
version: 1
output: pretty
tools:
  swiftlint:
    enabled: true
    check: false
    required: true
tasks:
  repository-check:
    name: Repository check (verify)
    command: pnpm
    args: [run, verify]
    required: true
```

If no composite gate exists, a root `typecheck` or `type-check` script is
imported as a change-aware `typecheck` task. This preserves Turborepo and
workspace-specific TypeScript semantics instead of replacing them with a raw
root `tsc` invocation.

Set `check: false` to keep an adapter available to `quality format` and
`quality fix` without also running it during `quality check`. Each adapter also
accepts `command`, `check_args`, `format_args`, and `fix_args`. Set
`working_directory` when a tool belongs to one workspace in a monorepo. This
provides an escape hatch for Gradle tasks, monorepo wrappers, and teams that pin
tools in a custom directory:

```yaml
version: 1
output: pretty
tools:
  detekt:
    enabled: true
    required: true
    working_directory: apps/android
    command: ./gradlew
    check_args: [detekt]
  ktlint:
    enabled: false
```

Set `required: false` to keep a locally optional tool from failing the run.

Repository-defined `tasks` preserve canonical gates such as type-checking,
package validation, tests, or builds. Tasks run during `quality check`, support
workspace directories, and can be skipped in changed-file mode when their
configured extensions and files are unaffected:

```yaml
tasks:
  typecheck:
    name: TypeScript
    command: pnpm
    args: [run, typecheck]
    extensions: [ts, tsx, astro]
    config_files: [package.json, tsconfig.json, pnpm-lock.yaml]
```

## Adopt incrementally with a baseline

For an existing repository with many findings, record the current state once:

```bash
quality baseline create
git add .quality-baseline.json quality.yml
```

After that, `quality check` suppresses matching existing findings and fails for
new ones. Fingerprints deliberately exclude line and column numbers, so moving
code does not create noise. Duplicate occurrences are counted, meaning a new
copy of an existing violation is still reported. Missing tools, crashes, and
unstructured execution failures can never be baselined.

Refresh intentionally after paying down findings:

```bash
quality baseline create --force
```

## Shell completions

Generate completions for Bash, Zsh, Fish, PowerShell, or Elvish. For example:

```bash
mkdir -p ~/.config/fish/completions
quality completions fish > ~/.config/fish/completions/quality.fish
```

The release workflow builds native archives for Linux, Apple Silicon and Intel
macOS, and Windows whenever a version tag such as `v0.4.0` is pushed.

Workflow generation requires an explicit installation command, preventing the
generated CI from assuming a crate or repository that does not exist. It
selects macOS for Swift repositories and Linux otherwise, then derives package
manager setup, dependency installation, and relevant native toolchain setup
from repository files, including Actionlint when its use is detected:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.4.0 --locked'
```

## Design direction

The stable core is deliberately small:

1. detect project ecosystems;
2. resolve native tools reproducibly;
3. execute independent checks concurrently;
4. normalize diagnostics and exit behavior;
5. integrate with CI through SARIF.

Future adapters can implement a documented plugin protocol. AI integrations
can later consume the same normalized diagnostics to explain or propose fixes,
without putting AI inside the deterministic checking path.

## Develop the monorepo

The repository keeps the Rust CLI and its website/documentation together:

```text
quality/
├── crates/quality-cli/  # Rust binary
├── apps/site/           # Astro + Starlight website and docs
├── Cargo.toml           # Cargo workspace
├── pnpm-workspace.yaml
└── turbo.json
```

Install workspace dependencies and use the shared commands:

```bash
pnpm install
pnpm dev    # Start the documentation site
pnpm check  # Rust and web checks
pnpm test   # Automated tests plus the disposable playground workflow
pnpm build  # Release CLI and production site
pnpm run ci # Affected-only pipeline used by GitHub Actions
```

Cargo remains available directly for Rust-only work. Turborepo coordinates
Cargo and Astro, caches deterministic tasks, and scopes CI to affected projects.

## Try features in the playground

The repository includes a disposable playground with mock analyzers, so CLI
features can be exercised without installing Swift, Android, or JavaScript
tooling. It requires the same Rust, Git, and POSIX shell tools used for local
development:

```bash
pnpm playground:setup
pnpm playground -- doctor
pnpm playground -- check
pnpm playground -- fix
pnpm playground -- format --check
pnpm playground -- format
```

The first `check` and `format --check` intentionally fail to demonstrate
diagnostics. The sandbox is a standalone Git repository, so changed-file mode,
baselines, JSON output, and SARIF reports work as they would in a real project.
See [`playground/README.md`](playground/README.md) for the complete walkthrough.

Run `pnpm playground:verify` to create a temporary sandbox and verify the whole
playground workflow automatically.

## License

MIT. See [LICENSE](LICENSE).
