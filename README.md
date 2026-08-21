# quality

`quality` is one fast, predictable code-quality workflow for repositories that
contain Swift, Android/Kotlin, and JavaScript projects. It does not replace the
ecosystem's best analyzers. It detects, runs, and explains them through one CLI.

Website and documentation: <https://quality-cli.santi020k.chatgpt.site>

> Status: early working prototype. The configuration format may change before
> the first stable release.

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

## Install for development

```bash
cargo install --path crates/quality-cli
```

After the repository is published and has a tagged release, Unix users can
install a checksum-verified native binary without Rust:

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/OWNER/REPOSITORY/main/install.sh \
  | sh -s -- OWNER/REPOSITORY
```

Pass a tag such as `v0.1.0` after the repository name to pin the installation.
Windows archives are attached to the same GitHub release.

Then run these commands from any repository:

```bash
quality init            # Write an explicit policy based on detected files
quality doctor          # Explain what is enabled, installed, or missing
quality check           # Run applicable linters concurrently
quality format          # Run applicable formatters
quality format --check  # Check formatting without modifying files
quality fix             # Apply fixes supported by the configured tools
quality baseline create # Record existing findings and block new regressions
quality completions zsh # Generate native shell completions
quality ci github --install '…' # Generate a runnable GitHub Actions workflow
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
a rules or configuration file also triggers the corresponding full check.

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
| Swift | SwiftLint | yes | fix |
| Swift | SwiftFormat | yes | yes |
| Android | Android Lint | yes | — |
| Kotlin | detekt | yes | — |
| Kotlin | ktlint | yes | yes |
| JavaScript/TypeScript | ESLint | yes | fix |
| JavaScript/TypeScript | Prettier | yes | yes |

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
project. Supported parsers are `generic`, `eslint-json`, `swiftlint-json`, and
`ktlint-json`. Generic diagnostics use the familiar format:

```text
path/to/file:line:column: warning: Message (rule-id)
```

External adapters participate in `doctor`, concurrent execution, changed-file
filtering, JSON/SARIF reports, GitHub annotations, and baselines. Commands are
executed directly without a shell, so arguments remain explicit and portable.

## Configuration

`quality init` generates a minimal file containing only detected tools:

```yaml
version: 1
output: pretty
tools:
  swiftlint:
    enabled: true
    required: true
  swiftformat:
    enabled: true
    required: true
```

Each adapter also accepts `command`, `check_args`, `format_args`, and
`fix_args`. This provides an escape hatch for Gradle tasks, monorepo wrappers,
and teams that pin tools in a custom directory:

```yaml
version: 1
output: pretty
tools:
  detekt:
    enabled: true
    required: true
    command: ./gradlew
    check_args: [detekt]
  ktlint:
    enabled: false
```

Set `required: false` to keep a locally optional tool from failing the run.

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
macOS, and Windows whenever a version tag such as `v0.1.0` is pushed.

Workflow generation requires an explicit installation command, preventing the
generated CI from assuming a crate or repository that does not exist:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v0.1.0 --locked'
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
pnpm test   # Rust test suite
pnpm build  # Release CLI and production site
pnpm ci     # Affected-only pipeline used by GitHub Actions
```

Cargo remains available directly for Rust-only work. Turborepo coordinates
Cargo and Astro, caches deterministic tasks, and scopes CI to affected projects.
