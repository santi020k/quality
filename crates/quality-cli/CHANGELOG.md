# @quality/cli

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
