# @quality/cli

## 0.4.0

### Minor Changes

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add bounded analyzer concurrency, configurable timeouts, retained-output limits,
  empty-policy protection, reliable fail-fast semantics, complete project
  discovery, atomic generated-file writes, and clearer GitHub Action operational
  failures.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add a built-in `santi-og` adapter that discovers `@santi020k/og` workspaces, checks generated Open Graph assets without modifying them, and normalizes stale outputs for terminal, JSON, SARIF, GitHub, and baseline reporting.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Detect installed CommitPrompt packages during initialization and preset generation, preserving existing commit-message policies, and enforce CSpell when validating the quality repository itself.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add minimal, recommended, and strict language-aware configuration presets for JavaScript, Rust, Swift, Kotlin, and GitHub Actions, including versioned upgrade metadata, safe diffs and updates, managed configuration merging, explicit framework packs, doctor compatibility reporting, and native setup guidance.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Version native JSON reports and publish their schemas, document stable contracts
  and exit codes, test portable behavior across Linux, macOS, and Windows, exercise
  representative real analyzers on a schedule, and verify the bundled Action
  against staged release assets before publication. Keep generated JavaScript
  presets internally consistent across ESLint, Prettier, CSpell, and Knip.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add first-class Codespell and Typos adapters with configuration detection, changed-file checks, safe fixes, normalized diagnostics, and single-spell-checker preset selection including Python projects.

### Patch Changes

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Publish the documentation and configuration schema from the public Cloudflare-hosted quality domain.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add opt-in repository audit failure policies for invalid configurations,
  missing configurations, and missing required toolchains.

## 0.3.1

### Patch Changes

- Add package-manager-independent Git hooks with conflict-safe installation,
  ordered steps from `quality.yml`, Commitprompt argument forwarding, status,
  and managed uninstall support.

- [#22](https://github.com/santi020k/quality/pull/22) [`745e38f`](https://github.com/santi020k/quality/commit/745e38ffc5ea87eca373f92bd83ebc80759b49ea) Thanks [@santi020k](https://github.com/santi020k)! - Identify each missing required toolchain in multi-repository adoption reports so rollout problems are actionable in both pretty and JSON output.

## 0.3.0

### Minor Changes

- [#20](https://github.com/santi020k/quality/pull/20) [`4635669`](https://github.com/santi020k/quality/commit/46356693441ca8c533ac91cdc05a0dca711d76fe) Thanks [@santi020k](https://github.com/santi020k)! - Add multi-repository adoption audit/apply commands with machine-readable reports, explicit fast/full initialization gates, aggregate diagnostic summaries, and improved SwiftPM and Xcode project detection.

## 0.2.1

### Patch Changes

- [#17](https://github.com/santi020k/quality/pull/17) [`7057458`](https://github.com/santi020k/quality/commit/70574589eed1aa1624263f419573816374b8321d) Thanks [@santi020k](https://github.com/santi020k)! - Improve rollout reliability by ignoring diagnostic-looking output from successful generic tasks, distinguishing disabled checks in doctor output, validating the Android Java runtime, generating spellcheck-safe configuration, warning when no checks run, and classifying environment and toolchain failures separately from code findings.

## 0.2.0

### Minor Changes

- [#15](https://github.com/santi020k/quality/pull/15) [`9d685c4`](https://github.com/santi020k/quality/commit/9d685c45db094bff5d044051dc3f6cf1b52e6324) Thanks [@santi020k](https://github.com/santi020k)! - Add deterministic AI-agent instructions, publish a `quality.yml` JSON Schema,
  and document the agent workflow through an AI-readable site index.

- [#15](https://github.com/santi020k/quality/pull/15) [`9d685c4`](https://github.com/santi020k/quality/commit/9d685c45db094bff5d044051dc3f6cf1b52e6324) Thanks [@santi020k](https://github.com/santi020k)! - Add adapter selection to checks, formatting, and fixes, record the selected
  scope in JSON and SARIF reports, and make changed-file mode account for deleted
  source and configuration paths without passing nonexistent files to tools.

- [#15](https://github.com/santi020k/quality/pull/15) [`9d685c4`](https://github.com/santi020k/quality/commit/9d685c45db094bff5d044051dc3f6cf1b52e6324) Thanks [@santi020k](https://github.com/santi020k)! - Make first-time adoption preserve canonical package checks, require explicit
  project intent before enabling JavaScript and Swift analyzers, deduplicate
  nested Swift workspaces, ignore generated dependency caches during discovery,
  and generate ecosystem-aware GitHub Actions setup with frozen dependency
  installation. Add built-in CSpell, Knip, and Actionlint adapters for checks
  already shared by the target repositories, plus an `init --dry-run` adoption
  preview.

## 0.1.2

### Patch Changes

- [#5](https://github.com/santi020k/quality/pull/5) [`7daa0c3`](https://github.com/santi020k/quality/commit/7daa0c3ef218d7c0ef8e9d0ea2375585201c243a) Thanks [@santi020k](https://github.com/santi020k)! - Generate GitHub workflows with SHA-pinned actions and safer checkout credentials.

## 0.1.1

## 0.1.0

### Minor Changes

- [`1d59f46`](https://github.com/santi020k/quality/commit/1d59f466d86748a52ac4c9159b7d7c13071881b6) Thanks [@santi020k](https://github.com/santi020k)! - Publish the first preview of the deterministic multi-ecosystem quality CLI and its checksum-verifying GitHub Action.
