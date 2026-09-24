---
title: GitHub Actions
description: Run changed-file checks with annotations, summaries, and SARIF.
---

The official Action installs a verified `quality` release, checks pull-request changes, and writes native annotations and a job summary:

```yaml
name: Quality

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read
  security-events: write

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  quality:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: santi020k/quality@v1.2.0
        id: quality
        with:
          version: v1.2.0
          changed-only: true
          report-level: warning
          fail-level: warning
          require-checks: true
          jobs: 4
          timeout-seconds: 120

      - name: Upload code-scanning results
        if: always() && steps.quality.outputs.sarif != ''
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: ${{ steps.quality.outputs.sarif }}
```

Pin `version` to a release for reproducible checks. Warnings fail by default; set `fail-level: error` only for an intentional, documented migration period.

The Action exposes `sarif`, `findings`, `tools`, and `duration-ms` outputs. On pull requests it compares against `origin/$GITHUB_BASE_REF`; on pushes without a base it safely checks the complete project.

It distinguishes policy findings from configuration or runtime failures so an
operational problem is not mislabeled as a diagnostic failure. `require-checks`
defaults to true; `jobs`, `timeout-seconds`, and `max-output-bytes` expose the
CLI's execution limits.

## Generate a standalone workflow

Generate a workflow with an explicit installation command:

```bash
quality ci github --install \
  'cargo install --git https://github.com/your-org/quality --tag v1.2.0 --locked'
```

The generated workflow:

1. Checks out complete Git history for change detection.
2. Selects macOS for Swift repositories and Linux otherwise.
3. Sets up the detected package manager and installs frozen dependencies.
4. Sets up detected Rust, Java/Gradle, SwiftLint, SwiftFormat, and Actionlint requirements.
5. Installs `quality` using the command you supplied.
6. Runs `quality doctor` to expose environment problems.
7. Checks pull-request changes against the base branch.
8. Emits native GitHub annotations and uploads SARIF to code scanning.

## Reuse pnpm CI

The repository also publishes a reusable pnpm workflow for build, test and browser
jobs. Generate a compact caller with the CLI:

```bash
quality ci github \
  --shared-ref '<reviewed-release-commit-sha>' \
  --command 'pnpm run verify'
```

Or call `.github/workflows/reusable-pnpm-ci.yml` directly from a job. The workflow
reads the pnpm version from `packageManager`, installs frozen dependencies, runs a
trusted repository command and can install cached Playwright browsers or upload
failure diagnostics. Repositories without a `packageManager` declaration must pass
the workflow's `pnpm-version` input; generated callers add a pinned fallback. Use a
caller matrix to shard browser jobs.

Deployment credentials, database migrations, environment approvals, tagging and
production smoke tests stay in the consuming repository. Deployment jobs can use
`santi020k/quality/actions/setup-pnpm` to remove repeated setup steps without moving
those safeguards into shared code. Package-release jobs can pass `registry-url`;
authentication tokens remain scoped to the consuming workflow's environment.

### Migrate an existing pnpm job

Replace separate pnpm, Node.js and frozen-install steps with the composite action.
Keep repository-specific commands, conditions, matrices, credentials and deployment
gates in the consuming workflow:

```yaml
- uses: santi020k/quality/actions/setup-pnpm@eec1701b98bcc0b76d36af288ee78a0369cd84cc # v1.1.1
  with:
    node-version-file: .node-version

- run: pnpm run verify
```

For Turborepo or another local task runner, the next release also supports an
opt-in task-output cache before the frozen install. The commit SHA in this example
must be replaced with the reviewed release commit that contains these inputs:

```yaml
- uses: santi020k/quality/actions/setup-pnpm@<reviewed-release-commit-sha>
  with:
    node-version-file: .node-version
    task-cache-path: .turbo/cache
    task-cache-key: quality
    task-cache-config-path: turbo.json
```

The cache key isolates operating system, architecture, Node configuration, job
namespace, dependency and task-runner configuration, and commit. Exact commits can
save new entries while restore prefixes reuse valid task outputs from earlier runs.
Use a distinct `task-cache-key` for jobs or matrix shards that must not share
outputs. Leave `task-cache-path` empty to disable this cache. The action exposes
`task-cache-hit` for job summaries or diagnostics.

### Publish to an authenticated package registry

Quality 1.1.1 added `registry-url` to the shared setup action. Pin the action to a
reviewed commit, configure the registry without a token, and expose the token only
to the publishing step:

```yaml
- uses: santi020k/quality/actions/setup-pnpm@eec1701b98bcc0b76d36af288ee78a0369cd84cc # v1.1.1
  with:
    node-version: 24
    registry-url: https://registry.npmjs.org

- name: Publish package
  run: pnpm publish
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

The setup action writes the registry configuration expected by `actions/setup-node`;
it does not read or own the credential. If dependency installation also requires a
private registry, provide `NODE_AUTH_TOKEN` to the setup action step so its built-in
frozen install can authenticate. Keep that secret in the consuming repository or
environment, and never pass it as an action input.

## Cost-aware CI

Run JavaScript, Android, Kotlin, and Rust jobs on Linux whenever platform requirements permit. Reserve macOS runners for Swift and Xcode work.

Cancel obsolete pull-request runs when a newer commit arrives:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

For monorepos, combine the Action's `changed-only` mode with Turborepo's affected-package selection so unchanged applications never start expensive jobs.

Before pushing, use the repository's configured local gate to catch
deterministic failures without consuming a failed hosted run:

```bash
quality ci plan
quality ci local
```

The plan distinguishes exact local command coverage from steps that require
GitHub-hosted actions, expressions, permissions, or services. Local success is
early feedback; keep the protected GitHub check for the exact pushed commit.

## Reporting and failure levels

Reporting and build policy are independent:

```bash
quality check --report-level warning --fail-level error
```

Warnings appear as annotations and in SARIF, while only errors fail the job. Required tools that are missing always fail regardless of severity settings.
