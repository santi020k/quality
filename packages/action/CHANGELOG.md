# @quality/action

## 2.1.0

### Minor Changes

- Add built-in `biome` and `oxlint` adapters. Both are detected from dependencies,
  package scripts, or their configuration files, prefer the workspace-local
  executable, and support changed-file runs. Biome checks, formats, and fixes;
  Oxlint checks and fixes. Choose the JavaScript stack per repository with
  `tools.<id>.enabled`, and avoid enabling Prettier and the Biome formatter for the
  same files.

  Add the `github-annotations` parser for GitHub workflow-command output, available
  to custom adapters, tasks, and hook steps.

  Prepare the Pawl integration (beta): repositories that use `@santi020k/pawl`
  or a `pawl.config.*` file no longer auto-enable the `biome`, `oxlint`,
  `eslint`, or `prettier` adapters, because Pawl already runs those engines
  against its own baseline. Enabling an adapter in `quality.yml` still runs it.

## 2.0.0

### Major Changes

- Consolidate the v2 development line with reliable analyzer failures, bounded output,
  local CI history and comparisons, and repository tool-version pins. Renamed
  configuration and file-type changes now select the required checks. Concurrent
  history writes preserve each run, and comparisons choose the preceding matching run.
  Timed analyzers stop their managed process group or tree on expiry and preserve
  Unix terminal input and signal behavior. A small Unix-only signal-handling dependency provides safe forwarding.

  Refresh the documentation site to Lumen 5 and compatible stable build dependencies.
  The development workspace requires Node.js 22.23.4 or newer. TypeScript 6 remains
  required by the Astro checker and Action bundler; the Action runtime remains Node 24.

  Migration: install Quality 2 before relying on `.quality-version`; older binaries
  ignore pins. Pins apply to the selected project root. Analyzer execution failures
  and incomplete structured reports now fail even with `--fail-level error` or an
  existing baseline. Fix the analyzer or increase `--max-output-bytes`; never accept
  an incomplete baseline. `baseline create` now accepts execution limits too.

  The Action defaults to the CLI version bundled with that Action release. Set
  `version: latest` explicitly only when deliberate automatic upgrades are wanted.
  Exact versions are validated and downloaded or cached binaries must match them.
  The `tools` output counts only executed adapters; private temporary reports keep
  counts accurate when public SARIF output is disabled. GitHub annotations and
  SARIF locations now resolve nested projects relative to the checkout.

  The installer hashes the requested archive directly and requires one matching
  SHA-256 entry. Other filenames, malformed or duplicate entries, and SHA-1 digests
  are rejected. Prepared release notes include migration guidance; immutable tags
  must match the qualified revision, and stable latest promotion waits for smoke
  verification. No application data migration is required. Recover published issues
  through a new patch release; never replace a published version tag.
- Add deterministic impact planning and strict result verification for dependency-aware validation pipelines. Plans explain selected checks, prerequisites, independent execution batches, and conservative full-validation fallbacks without executing repository commands.

  Migration: configure the separate `quality-impact.json` contract to adopt impact planning. Existing `quality.yml`, `quality check`, and `quality ci plan` retain their meanings. The Action and CLI move together to version 2; update pinned release references when adopting the published release.

  Render changelogs locally from reviewed Changesets without network-dependent enrichment. Keep full release gates during impact-plan qualification; deployment and publication remain protected workflow operations.

### Patch Changes

- Add an additive `qualityExecuted` property to each SARIF run, marking whether
  the tool's process actually started. A missing required tool still produces a
  visible SARIF run and diagnostic, but is no longer counted in the Action's
  `tools` output, which now reflects adapters that ran rather than every
  reported run.

  Fix a tool-cache collision in the Action: it resolved the release repository
  and base URL after already checking the cache under a fixed `quality` key, so
  a fork or fixture server publishing the same version string could silently
  reuse another source's cached binary. The cache is now namespaced by the
  resolved release base URL and looked up only after that URL is resolved.
- Parse structured analyzer output independently from stderr and refuse incomplete reports even after a successful exit. Bound fallback diagnostics, preserve complete alternate-format overrides, and ignore informational typos binary-file events.

  Migration: generic `style` findings now use the published `info` severity. Review and recreate baselines containing these findings because their fingerprints change. Unreadable structured reports fail with `tool-output-unreadable`; fix the analyzer or increase the capture limit before accepting a baseline.

  Retain 20 metadata-only local CI runs per hook, compare matching executed commands and directories by default, and omit duration deltas for skipped steps. Tail capture preserves complete line boundaries and retries interrupted reads.

## 1.4.0

### Minor Changes

- Add `quality check --dry-run` and optional `--explain` output to preview commands,
  workspace scope, changed inputs, and selection or skip reasons without executing
  analyzers. Preserve existing execution and report contracts.

- Refresh the documentation with Lumen 4, Quality's blue identity, responsive navigation, accessible code examples, page transitions, and complete guide navigation. Upgrade compatible dependencies and address known dependency vulnerabilities. Preserve operational CLI errors in the GitHub Action when no SARIF report was written, and restrict release and deployment workflows to merged main commits. Remove an unused internal helper without changing stable CLI contracts. Pilot affected repository validation with full release checks, conservative fallbacks, and recorded local benchmarks. Patch audited transitive dependencies and rebuild the Action distribution. Align the private development workspace with its existing Node.js 22.19.0 dependency minimum.

## 1.3.0

No changes in this release.

## 1.2.0

No changes in this release.

## 1.1.2

### Patch Changes

- [#47](https://github.com/santi020k/quality/pull/47) [`67fddb5`](https://github.com/santi020k/quality/commit/67fddb58e1fdcd1779685744498854d65e8b9567) Thanks [@santi020k](https://github.com/santi020k)! - Add opt-in task-output caching to the shared pnpm action and reusable workflow, document authenticated registry publishing with tested consumer examples, and improve repeat site-build and deployment caching.

## 1.1.1

### Patch Changes

- [#45](https://github.com/santi020k/quality/pull/45) [`79b24cb`](https://github.com/santi020k/quality/commit/79b24cb89f9ad25b0abd4d411aaaa3d227745081) Thanks [@santi020k](https://github.com/santi020k)! - Allow the shared pnpm setup action to configure a package registry without moving publishing credentials out of the consuming workflow.

## 1.1.0

### Minor Changes

- [#40](https://github.com/santi020k/quality/pull/40) [`ab680e7`](https://github.com/santi020k/quality/commit/ab680e798348d377ab43eb85e2bc677d3c46a985) Thanks [@santi020k](https://github.com/santi020k)! - Add a reusable pnpm CI workflow, a composite pnpm workspace setup action and a CLI generator mode for thin, version-pinned caller workflows.

## 1.0.0

### Major Changes

- [#30](https://github.com/santi020k/quality/pull/30) [`e0e2881`](https://github.com/santi020k/quality/commit/e0e2881572c5a6d6ffab295a8b93feb9ce1872fc) Thanks [@santi020k](https://github.com/santi020k)! - Graduate quality to its first stable major release. Version 1 commits to the
  documented CLI, `quality.yml`, native report-schema, exit-code, and GitHub
  Action contracts while retaining the deterministic, AI-free checking path.

## 0.4.0

### Minor Changes

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add bounded analyzer concurrency, configurable timeouts, retained-output limits,
  empty-policy protection, reliable fail-fast semantics, complete project
  discovery, atomic generated-file writes, and clearer GitHub Action operational
  failures.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add minimal, recommended, and strict language-aware configuration presets for JavaScript, Rust, Swift, Kotlin, and GitHub Actions, including versioned upgrade metadata, safe diffs and updates, managed configuration merging, explicit framework packs, doctor compatibility reporting, and native setup guidance.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Version native JSON reports and publish their schemas, document stable contracts
  and exit codes, test portable behavior across Linux, macOS, and Windows, exercise
  representative real analyzers on a schedule, and verify the bundled Action
  against staged release assets before publication. Keep generated JavaScript
  presets internally consistent across ESLint, Prettier, CSpell, and Knip.

## 0.3.1

### Patch Changes

- [#22](https://github.com/santi020k/quality/pull/22) [`745e38f`](https://github.com/santi020k/quality/commit/745e38ffc5ea87eca373f92bd83ebc80759b49ea) Thanks [@santi020k](https://github.com/santi020k)! - Identify each missing required toolchain in multi-repository adoption reports so rollout problems are actionable in both pretty and JSON output.

## 0.3.0

### Minor Changes

- [#20](https://github.com/santi020k/quality/pull/20) [`4635669`](https://github.com/santi020k/quality/commit/46356693441ca8c533ac91cdc05a0dca711d76fe) Thanks [@santi020k](https://github.com/santi020k)! - Add multi-repository adoption audit/apply commands with machine-readable reports, explicit fast/full initialization gates, aggregate diagnostic summaries, and improved SwiftPM and Xcode project detection.

## 0.2.1

### Patch Changes

- [#17](https://github.com/santi020k/quality/pull/17) [`7057458`](https://github.com/santi020k/quality/commit/70574589eed1aa1624263f419573816374b8321d) Thanks [@santi020k](https://github.com/santi020k)! - Improve rollout reliability by ignoring diagnostic-looking output from successful generic tasks, distinguishing disabled checks in doctor output, validating the Android Java runtime, generating spellcheck-safe configuration, warning when no checks run, and classifying environment and toolchain failures separately from code findings.

## 0.2.0

## 0.1.2

## 0.1.1

### Patch Changes

- [#8](https://github.com/santi020k/quality/pull/8) [`06998e5`](https://github.com/santi020k/quality/commit/06998e5316700ff7f58ec02f018dec00e994036f) Thanks [@santi020k](https://github.com/santi020k)! - Support release verification when the Action runs from a local checkout.

## 0.1.0

### Minor Changes

- [`1d59f46`](https://github.com/santi020k/quality/commit/1d59f466d86748a52ac4c9159b7d7c13071881b6) Thanks [@santi020k](https://github.com/santi020k)! - Publish the first preview of the deterministic multi-ecosystem quality CLI and its checksum-verifying GitHub Action.
