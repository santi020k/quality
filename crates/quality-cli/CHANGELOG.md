# @quality/cli

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
