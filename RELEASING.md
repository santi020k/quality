# Releasing

## Repository prerequisites

Before relying on the automated release path, verify these external settings
without exposing their values:

- The Release PR workflow can create pull requests. Either enable **Allow
  GitHub Actions to create and approve pull requests** in the repository's
  Actions settings or provide a fine-grained `CHANGESETS_TOKEN` Actions secret
  with Contents and Pull requests read/write access.
- The repository's Infisical OIDC identity can read the `prod` environment at
  `/github/deploy-site`, where `CLOUDFLARE_ACCOUNT_ID` and
  `CLOUDFLARE_API_TOKEN` are shared secrets with only the permissions needed to
  deploy `quality-site`. The site workflow fails closed when either is missing.

## Prepare a release

1. Fetch refs, inspect worktree ownership, and select the intended version before
   branching from current `main`. Consolidate finished work into
   `release/v<semver>`; retain existing user changes and audit squash-merged
   branches for equivalent changes before retiring them.
2. Add a Changeset for user-visible changes. The CLI and Action are a fixed
   version group and must remain on the same version. For this candidate, the
   target is `2.1.0`; the documentation package has its own version.
3. Run `pnpm changeset status` and review the proposed versions. Run the documented
   `pnpm version-packages` generator on the release branch to update Cargo,
   installation examples, compatibility references, and changelogs together.
   Inspect every generated change; do not hand-edit generated version references.
4. Run `pnpm validate`, then `pnpm test:site` against the production build.
   Install the test browser with `pnpm --filter @quality/site exec playwright
   install chromium` when needed. In CI use `--with-deps`; local Chrome may be
   selected with `PW_CHROMIUM_CHANNEL=chrome`.
5. Obtain a fresh independent review of the complete diff against `main` before
   the first push. Resolve findings and rerun affected checks. Pushing or opening
   the release pull request requires explicit authorization.
6. Open the release pull request from `release/v<semver>` into `main`. Include
   release notes, migration requirements, validation evidence, and recovery steps.
   Merge only after required checks and all actionable reviews are satisfied.

The Release PR workflow can still generate version proposals from pending
Changesets on `main`. For a planned release, keep version preparation in the
selected release branch rather than creating a competing version proposal.

## Publish through GitHub

1. The merged version change on `main` starts the Release workflow. Manual
   dispatch is also limited to `main`; a release branch cannot publish artifacts
   or deploy the site.
2. The workflow validates the repository, builds and exercises every native
   archive, verifies the bundled Action against staged assets, publishes
   checksums and provenance attestations, creates the GitHub release, and tests
   the published Action. Standard GitHub-hosted macOS runners build Apple
   binaries for this public repository.
3. Confirm `vX.Y.Z`, each distributed artifact, and the moving major Action tag
   (for example `v1`) resolve to the intended merged commit.
4. Confirm the checksum-verifying installer, exact-version Action example, and
   deployed documentation against the published tag. Check homepage, docs,
   mobile navigation, theme changes, code tabs, and internal page transitions.
5. Only after post-release verification, delete the release branch remotely and
   locally, and return to an up-to-date local `main`. Remote changes require the
   user's authorization.

Production documentation waits for a published stable release whose immutable
tag is an ancestor of the selected main revision, with successful published
Action smoke and stable promotion jobs. Release completion automatically retries
the site workflow; docs-only main changes can deploy against that verified
release. The source SHA is checked again before loading deployment secrets. Every main
push evaluates a deployment candidate so a source-only commit cannot drop an
older pending documentation update. This may rebuild the site for source-only
changes until narrower selection is qualified.

An unmerged or untagged deliberate `release/v*` branch suppresses automated
version PR preparation. Consume every Changeset on that branch before merging.
If publication succeeds but smoke fails, rerun the original release workflow at
its original SHA. Recovery checks preserve newer major aliases and the global
latest release; a backport can advance its own major alias. Equal-precedence
versions with different commits or build identities fail rather than choosing
between them. Missing metadata or API failures block promotion. A code fix
requires a new version; do not move the existing version tag.

## Migration and recovery for 2.0.0

The CLI and Action move together to major version 2. The impact graph is an
explicit separate `quality-impact.json` contract; existing analyzer configuration
and `quality ci plan` retain their meanings. See [Impact pipelines](docs/impact-pipelines.md).
Existing consumers should review these v2 changes before updating:

- Install Quality 2 before relying on `.quality-version`. Pins apply to the
  selected project root; older binaries ignore them.
- Unparsed analyzer failures and incomplete structured output remain failures
  even with a baseline or a higher severity threshold. Fix the tool or raise the
  capture limit. Baseline creation accepts the same execution limits as checks.
- Structured parsers read standard output independently of standard error.
  Unreadable structured reports fail with `tool-output-unreadable`, including
  after a successful process exit. Complete alternate-format overrides remain
  supported. Generic `style` findings now use the published `info` severity;
  review and recreate baselines containing those findings because their
  fingerprints change.
- Local CI retains 20 metadata-only runs per hook. Default comparisons require
  matching executed commands and working directories; pass an explicit base
  to compare different workloads. Skipped steps have no duration delta.
- The Action defaults to its own packaged CLI version. `version: latest` is an
  explicit opt-in, exact versions are verified, and `tools` counts only executed
  adapters. Counts remain accurate with public SARIF output disabled.
- The installer requires exactly one SHA-256 entry naming the requested archive.
  Invalid, duplicate, wrong-algorithm, and unrelated entries are rejected.

No application-data migration is required. Keep full release validation while
qualifying impact selection, and retain the existing pre-push and release gates.

The development workspace now requires Node `>=22.23.4` for the documentation
site's patched Astro 7.4.12 dependency and matching Markdown processor 0.4.10.
This is a development and site-build
requirement; the native CLI and the Action's Node 24 runtime are unchanged.
Astro 7.4.12 includes the fixes for
[the image endpoint path check](https://github.com/withastro/astro/security/advisories/GHSA-r3h5-4585-7x3w)
and [dynamic tag attribute injection](https://github.com/withastro/astro/security/advisories/GHSA-mh32-jxcr-mh9f).
Its [release notes](https://github.com/withastro/astro/releases/tag/astro%407.4.12)
also document reduced content-collection memory use during builds. Keep the site
manifest and lockfile together, and verify diagnostics, the production build,
browser behavior, and the dependency audit after updates.

Release notes now use the local deterministic Changesets renderer. Changesets
still owns version propagation; the renderer never queries GitHub during version
preparation. After publication, consumers must update reviewed exact-version or
commit pins deliberately. Do not point workflows at an unpublished v2 binary.

If planning or verification fails, run the full canonical gate and fix the
configuration; do not accept missing results. Recovery after publication follows
a new patch release through GitHub Actions. Never replace the v2.0.0 tag.

## Migration and recovery for 1.4.0

CLI flags, configuration, reports, exit codes, and Action inputs remain compatible
with 1.x. Documentation URLs and machine-readable Markdown endpoints are retained.
The site consumes published Lumen 4 packages; no local package links are required.

### Dependency security follow-up — 2026-10-25

The affected-validation pilot initially exposed 16 existing audit findings
(5 high, 9 moderate, 2 low). The workspace now pins patched transitive versions
within each consumer's existing major version, and `pnpm check:security` reports
no known vulnerabilities. These changes remain part of the prepared 1.4.0
candidate; the generated release notes already include dependency remediation.

| Dependency | Previous version | Patched version | Upstream security notes |
| --- | --- | --- | --- |
| launch-editor | 2.14.1 | 2.14.2 | [Windows argument escaping](https://github.com/advisories/GHSA-698v-4r3x-3vcv), [editor flag injection](https://github.com/advisories/GHSA-r825-w8mm-5pxw), [Windows file validation](https://github.com/advisories/GHSA-587h-7wmj-h5pm) |
| Zod | 4.6.5 | 4.6.7 | [Collection bounds fix in 4.6.6](https://github.com/colinhacks/zod/releases/tag/v4.6.6) |
| Undici | 6.29.0 / 7.29.1 / 8.11.2 | 6.29.1 / 7.30.1 / 8.12.3 | [6.x fixes](https://github.com/nodejs/undici/releases/tag/v6.29.1), [7.x fixes](https://github.com/nodejs/undici/releases/tag/v7.30.1), [8.x security fixes](https://github.com/nodejs/undici/releases/tag/v8.12.2) and [follow-up fixes](https://github.com/nodejs/undici/releases/tag/v8.12.3) |
| fast-uri | 3.1.8 | 3.1.9 | [URI parsing security fixes](https://github.com/fastify/fast-uri/releases/tag/v3.1.9) |

Versions and runtime requirements were checked against the official npm
registry and upstream security notes. Miniflare pins its Undici dependency
exactly, so a normal lockfile refresh cannot resolve that finding. The bounded
overrides in `pnpm-workspace.yaml` prevent vulnerable versions from returning
without crossing dependency majors. Remove an override when every upstream
consumer permits a patched version, then regenerate the lockfile and require a
clean audit and full validation. The Action distribution is rebuilt to include
Undici 6.29.1; updating the lockfile alone does not patch the shipped bundle.

### Rust audit and workflow follow-up

The CLI now uses the YAML organization's maintained `yaml_serde` 0.10.7 with
`libyaml-rs`, removing deprecated `serde_yaml` and unmaintained `unsafe-libyaml`.
The [upstream migration guide](https://github.com/yaml/yaml-serde#migrating-from-serde_yaml)
documents the compatible API. Rust 1.85 compilation and regression tests cover
configuration validation, aliases, quoted scalars, multiline commands, tags,
workflow triggers, serialization, and existing CLI behavior.

Rust CI runs pinned `cargo-audit` 0.22.2 directly with `--deny warnings`, retaining
a read-only token and avoiding the old Action's deprecated Node runtime and
failed Checks API writes. The live advisory database is not cached. CodeQL now
also scans pushes to `main` so PR comparisons have a current default-branch
baseline.

The upstream `http-cache-semantics` Scorecard warning remains visible. Version
4.3.0 is the latest stable release and is required by Astro; even current Astro
still depends on it. Its upstream OpenSSF score is 2.9 (2026-10-19), below the
existing warning threshold of 3, because of upstream review, branch protection,
security policy, fuzzing, and static analysis practices. Quality cannot change
those upstream controls. No exclusion or lower threshold is added. Track this
warning in PR #56 and reassess upstream releases and the score before rollout;
removing Astro's caching dependency would require a separate compatibility review.

### Runtime and recovery

The private development workspace now declares Node `>=22.19.0`, matching
Astro's existing Undici 8 requirement. Contributors on Node 22.18 must update
their development runtime. Native CLI compatibility and the Action's Node 24
runtime are unchanged. Local validation uses Node 22.23.1; CI uses Node 24.

There is no persisted-data migration. If validation or publication fails, fix the
candidate and rerun GitHub Actions. If an issue is discovered after publication,
prepare a new patch release through the same workflow; never move or replace a
published version tag. Consumers can pin the preceding exact release while the
patch is prepared. A site recovery should revert the affected site change in a
reviewed follow-up commit and use the normal deployment workflow from `main`.

Keep local validation, GitHub workflow results, published assets, the promoted
major tag, and deployed documentation as separate evidence when reporting status.
