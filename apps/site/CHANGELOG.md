# @quality/site

## 0.4.0

### Minor Changes

- Add structured diagnostics to local hook failures, enforce optional `.quality-version` pins, and provide versioned metadata-only Local CI history, show, and comparison commands with published JSON schemas.
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

### Patch Changes

- Upgrade the documentation site to Astro 7.4.12, including the fixes for
  GHSA-r3h5-4585-7x3w and GHSA-mh32-jxcr-mh9f. The development workspace and site
  build now require Node 22.23.4 or later; native CLI compatibility and the Action's
  Node 24 runtime are unchanged.

## 0.3.1

### Patch Changes

- Keep mobile navigation responsive while its dialog controller downloads by using native dialog commands. Verify early pointer and keyboard activation, dismissal, and the loaded Lumen fallback.

## 0.3.0

### Minor Changes

- Refresh the documentation with Lumen 4, Quality's blue identity, responsive navigation, accessible code examples, page transitions, and complete guide navigation. Upgrade compatible dependencies and address known dependency vulnerabilities. Preserve operational CLI errors in the GitHub Action when no SARIF report was written, and restrict release and deployment workflows to merged main commits. Remove an unused internal helper without changing stable CLI contracts. Pilot affected repository validation with full release checks, conservative fallbacks, and recorded local benchmarks. Patch audited transitive dependencies and rebuild the Action distribution. Align the private development workspace with its existing Node.js 22.19.0 dependency minimum.

### Patch Changes

- Keep the homepage heading inside its container with enlarged text and wider platform fonts. Strengthen browser coverage to verify text enlargement and heading containment before checking page overflow.

## 0.2.0

### Minor Changes

- [#53](https://github.com/santi020k/quality/pull/53) [`5224439`](https://github.com/santi020k/quality/commit/5224439c6dcbf1d79670dec192cb60fd5e61359e) Thanks [@santi020k](https://github.com/santi020k)! - Add compact, bounded `agent` output for checks and doctor reports, with file-grouped diagnostics, separated environment failures, focused rerun commands, and documentation for AI coding-agent workflows.

## 0.1.0

### Minor Changes

- [#51](https://github.com/santi020k/quality/pull/51) [`11403a8`](https://github.com/santi020k/quality/commit/11403a8a68d8b7838e0fddf2a0f92dc17598672b) Thanks [@santi020k](https://github.com/santi020k)! - Add local CI planning and execution for configured Git hooks, including pull-request workflow coverage, per-step and total wall time, bounded failure output, focused reruns, metadata-only Git history, versioned JSON reports, and published schemas.

## 0.0.4

### Patch Changes

- [#29](https://github.com/santi020k/quality/pull/29) [`974f82d`](https://github.com/santi020k/quality/commit/974f82d88e90939189fc3804024b199f3d6aed6e) Thanks [@santi020k](https://github.com/santi020k)! - Update installation, GitHub Action, and generated workflow examples to use the
  current 0.4.0 release.

- [#30](https://github.com/santi020k/quality/pull/30) [`e0e2881`](https://github.com/santi020k/quality/commit/e0e2881572c5a6d6ffab295a8b93feb9ce1872fc) Thanks [@santi020k](https://github.com/santi020k)! - Graduate quality to its first stable major release. Version 1 commits to the
  documented CLI, `quality.yml`, native report-schema, exit-code, and GitHub
  Action contracts while retaining the deterministic, AI-free checking path.

## 0.0.3

### Patch Changes

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add bounded analyzer concurrency, configurable timeouts, retained-output limits,
  empty-policy protection, reliable fail-fast semantics, complete project
  discovery, atomic generated-file writes, and clearer GitHub Action operational
  failures.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Detect installed CommitPrompt packages during initialization and preset generation, preserving existing commit-message policies, and enforce CSpell when validating the quality repository itself.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add minimal, recommended, and strict language-aware configuration presets for JavaScript, Rust, Swift, Kotlin, and GitHub Actions, including versioned upgrade metadata, safe diffs and updates, managed configuration merging, explicit framework packs, doctor compatibility reporting, and native setup guidance.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Publish the documentation and configuration schema from the public Cloudflare-hosted quality domain.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Version native JSON reports and publish their schemas, document stable contracts
  and exit codes, test portable behavior across Linux, macOS, and Windows, exercise
  representative real analyzers on a schedule, and verify the bundled Action
  against staged release assets before publication. Keep generated JavaScript
  presets internally consistent across ESLint, Prettier, CSpell, and Knip.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add first-class Codespell and Typos adapters with configuration detection, changed-file checks, safe fixes, normalized diagnostics, and single-spell-checker preset selection including Python projects.

- [#24](https://github.com/santi020k/quality/pull/24) [`c7ddf51`](https://github.com/santi020k/quality/commit/c7ddf513ad1ff3022e2b1f8b18ae7bdbf4927698) Thanks [@santi020k](https://github.com/santi020k)! - Add opt-in repository audit failure policies for invalid configurations,
  missing configurations, and missing required toolchains.

## 0.0.2

### Patch Changes

- [#22](https://github.com/santi020k/quality/pull/22) [`745e38f`](https://github.com/santi020k/quality/commit/745e38ffc5ea87eca373f92bd83ebc80759b49ea) Thanks [@santi020k](https://github.com/santi020k)! - Publish synchronized AI-readable documentation through `llms.txt`, `llms-full.txt`, a compatibility `llm.txt`, and per-page Markdown endpoints.
