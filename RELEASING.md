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
   target is `1.4.0`; the documentation package has its own version.
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

### Runtime and recovery

The inherited workspace Node floor needs confirmation before publication:
`package.json` still declares `>=22.18.0`, while Astro's existing Undici 8
dependency requires `>=22.19.0`. This candidate preserves the declared floor
pending that decision. Local validation uses Node 22.23.1; CI uses Node 24.

There is no persisted-data migration. If validation or publication fails, fix the
candidate and rerun GitHub Actions. If an issue is discovered after publication,
prepare a new patch release through the same workflow; never move or replace a
published version tag. Consumers can pin the preceding exact release while the
patch is prepared. A site recovery should revert the affected site change in a
reviewed follow-up commit and use the normal deployment workflow from `main`.

Keep local validation, GitHub workflow results, published assets, the promoted
major tag, and deployed documentation as separate evidence when reporting status.
