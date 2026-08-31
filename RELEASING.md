# Releasing

## Repository prerequisites

Before relying on the automated release path, verify both of these external
settings without exposing their values:

- The Release PR workflow can create pull requests. Either enable **Allow
  GitHub Actions to create and approve pull requests** in the repository's
  Actions settings or provide a fine-grained `CHANGESETS_TOKEN` Actions secret
  with Contents and Pull requests read/write access.
- The repository's Infisical OIDC identity can read the `prod` environment at
  `/github/deploy-site`, where `CLOUDFLARE_ACCOUNT_ID` and
  `CLOUDFLARE_API_TOKEN` are shared secrets with only the permissions needed to
  deploy `quality-site`. The site workflow fails closed when either is missing.

## Prepare a release

1. Add a Changeset for every user-visible change. The CLI and Action are a
   fixed version group and must remain on the same version.
2. Run `pnpm changeset status` and confirm the proposed versions. For the first
   stable release, both `@quality/cli` and `@quality/action` must resolve to
   `1.0.0`.
3. Run `pnpm validate`. This includes release-metadata agreement, the generated
   Action bundle, security checks, tests, and production builds.
4. Merge the preparation pull request. Do not run `version-packages` merely to
   prepare the release and do not create a release tag manually.

## Publish through the release pull request

1. The Release PR workflow maintains `chore(release): version packages`. Its
   version script updates Cargo, installation examples, compatibility language,
   and changelogs together.
2. Review the generated versions, public documentation, and changelogs, then
   merge that pull request.
3. The Release workflow validates the repository, builds and exercises every
   native archive, verifies the bundled Action against staged assets, publishes
   checksums and provenance attestations, creates the GitHub release, and tests
   the published Action.
4. For stable releases, confirm the moving major Action tag (for example `v1`)
   resolves to the exact release tag.
5. Confirm the checksum-verifying installer, exact-version Action example, and
   deployed documentation against the published tag.

The workflow creates `vX.Y.Z` only after every native archive and the staged
Action pass. Keep local validation, published GitHub assets, the promoted major
tag, and the deployed documentation as separate release states when reporting
progress.
