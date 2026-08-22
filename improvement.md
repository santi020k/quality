# Possible improvements

This document records improvement opportunities found while verifying the
`v0.3.0` rollout on 2026-08-22. The parent-folder audit found 24 Git
repositories, all 24 configured with `quality.yml`, no missing configurations,
and no invalid configurations. One repository, `ContracTrack`, currently
reports a missing local Android toolchain (`android-lint@apps/android`). That is
an environment-readiness issue rather than an incomplete `v0.3.0` rollout.

## Highest priority

### 1. Give repository audits useful exit codes

`repositories audit` currently prints missing configurations, invalid
configurations, and unavailable required toolchains but still exits
successfully. Add an opt-in strict policy such as `--fail-on invalid`,
`--fail-on missing-configuration`, or `--fail-on missing-toolchain`, with a
documented default. This would make the audit reliable in scheduled jobs and CI
without requiring consumers to parse JSON.

Acceptance criteria:

- Pretty and JSON output retain the same findings.
- Strict mode returns a non-zero exit code for the selected conditions.
- Each condition has an integration test.

### 2. Separate configuration readiness from machine readiness

The adoption report currently assigns one status to each repository. A valid
repository can therefore appear not ready only because the auditing machine
lacks Java, SwiftLint, or another local dependency. Report two explicit states:
configuration readiness and toolchain readiness. Include the missing executable
or prerequisite and a suggested installation or setup action where it can be
determined safely.

This would make results such as the current `ContracTrack` Android warning more
actionable and prevent environment problems from being confused with rollout
gaps.

### 3. Pin every generated GitHub Action to an immutable commit

Generated workflows pin several actions by commit, but the Bun and Java setup
steps still use mutable major-version tags. Resolve and pin those actions in the
template generator, keep the readable version comment, and add a test that
rejects generated `uses:` entries that are not full commit SHAs. This aligns the
generator with the repository's stated supply-chain posture.

### 4. Detect configuration drift

The audit validates an existing `quality.yml`, but it does not explain when the
repository's ecosystems, package scripts, or analyzer configuration have
changed since initialization. Add `repositories audit --drift` and a
single-repository equivalent that compare the current policy with a freshly
detected policy without overwriting user choices.

The report should distinguish:

- newly detected adapters or workspaces;
- configured adapters whose underlying project intent disappeared;
- newly available canonical gates;
- intentional overrides that should remain untouched.

## Adoption workflow

### 5. Support gate selection in bulk apply

`quality init` supports `--gate auto|fast|full`, while `repositories apply`
always uses the automatic profile. Add the same option to bulk application and
record the selected profile in JSON output. This keeps one-repository and
multi-repository adoption behavior consistent.

### 6. Make repository discovery configurable

Bulk discovery currently considers only direct children whose `.git` path is a
directory. Add support for Git worktrees, where `.git` can be a file, and
optional recursive discovery for grouped project folders. Include `--include`,
`--exclude`, and a bounded `--max-depth` so large development folders remain
predictable and fast.

### 7. Add a configuration migration command

Before the configuration schema advances beyond version 1, provide
`quality config migrate --dry-run` and `quality config validate`. Migrations
should preserve comments and explicit user choices when possible, show a diff,
and never silently rewrite a policy during `check`.

### 8. Version the machine-readable report contracts

Add a `schema_version` field to JSON reports and publish JSON Schemas for run,
doctor, and adoption output. The CLI, GitHub Action, and future integrations can
then evolve without forcing consumers to infer compatibility from the binary
version.

## Architecture and extensibility

### 9. Introduce the documented adapter protocol

The README identifies a plugin protocol as the future design direction. Define
a small versioned protocol for discovery, operations, changed-file behavior,
diagnostic parsing, and capability reporting. Start with an experimental flag
and a conformance test kit so external adapters cannot compromise deterministic
checking.

### 10. Split large execution and adapter modules

`tools.rs` and `runner.rs` contain most of the CLI implementation. Split built-in
adapters by ecosystem and separate process execution, scheduling, parsing, and
report aggregation. This will reduce regression risk as more adapters and
report formats are added without changing public behavior.

### 11. Add cancellation and process cleanup

For fail-fast runs and interrupted commands, ensure all spawned analyzer process
trees are terminated on every supported platform. Add tests with long-running
mock tools to verify cancellation, cleanup, and deterministic partial reports.

### 12. Improve cache-aware execution

Add an optional local cache keyed by the quality configuration, adapter version,
relevant config files, operation, and input file hashes. Keep caching disabled
by default initially, expose cache hits in structured output, and never reuse
results when tool identity cannot be established.

## Testing and release quality

### 13. Expand cross-platform integration coverage

Add explicit tests for Windows paths and process invocation, Git worktrees,
repositories with spaces or non-ASCII names, nested workspaces, symlinks, and
mixed package managers. Keep fixture analyzers deterministic so the test suite
does not depend on globally installed tools.

### 14. Add property and snapshot tests for public output

Property tests would help cover changed-file matching, baseline fingerprinting,
path normalization, and adapter selection. Reviewed snapshots for pretty, JSON,
SARIF, GitHub, doctor, and adoption output would make accidental contract
changes visible.

### 15. Keep release metadata current

The README still labels the project as a `0.1.x` preview, and the top-level
changelog's Unreleased section still refers to preparing `0.1.0`. Update these
as part of each release and add a release check that verifies preview status,
examples, package versions, action versions, and changelog text agree.

### 16. Test installed artifacts end to end

Extend release validation beyond `quality --version`: unpack every generated
archive, run `init`, `doctor`, a passing check, a failing check, and JSON/SARIF
serialization against fixtures. Keep the published GitHub Action smoke test and
verify checksum and provenance instructions on each supported operating system.

## Documentation and user experience

### 17. Publish task-oriented troubleshooting

Add concise guides for unavailable Java/Gradle, Swift tools, Node package
manager mismatches, invalid Git revisions in changed mode, and analyzer output
that cannot be parsed. Each doctor or runtime error should link or point to a
stable troubleshooting identifier.

### 18. Add explain and plan views

Provide commands such as `quality explain <adapter>` and
`quality check --plan`. They should show why an adapter was detected, which
executable and arguments will run, its working directory, whether it is
file-scoped or project-scoped, and which changed files triggered it. This would
make generated policies easier to trust without running the analyzers.

### 19. Report audit changes over time

Allow adoption JSON reports to be compared so teams can see newly configured,
newly invalid, or newly missing-toolchain repositories. A deterministic
`repositories diff <before.json>` command would support fleet maintenance
without adding network access or AI to the checking path.

## Suggested sequence

1. Audit exit codes and separate readiness states.
2. Immutable Action pins and release-metadata checks.
3. Drift detection and bulk gate selection.
4. Report schemas and configuration migration.
5. Repository discovery improvements and cross-platform tests.
6. Adapter protocol, internal module split, and optional caching.

All of these improvements should preserve the core constraint: checks remain
deterministic and AI-free, while normalized reports may be consumed by optional
AI-assisted tooling outside the checking path.
