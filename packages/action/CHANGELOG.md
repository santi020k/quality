# @quality/action

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
