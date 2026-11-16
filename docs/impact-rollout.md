# Impact rollout across sibling projects

Inventory checked on 2026-11-16 against top-level sibling manifests, local Git
refs, and `.github/workflows/`. This is a local implementation inventory, not
evidence of publication, deployment, or completion across all repositories.
Backup directories and duplicate scratch checkouts are excluded.

## Implemented pilots

These are the selected release targets for this work. Integration, publication,
and production verification must be reported separately from implementation.

| Project and selected release | Implemented change | Remaining adoption |
| --- | --- | --- |
| Quality `release/v2.0.0` | Deterministic impact plan and result verification, repository policy, shadow CI plan, owned Changesets renderer. Existing full checks remain required. | Publish through the normal release workflow, verify the binary and Action, then pin consumers to the verified release revision. |
| Lumen `release/v5.3.0` | Classify Apple inputs before expensive native jobs; preserve stable checks and require full Apple qualification for release/signing. Shared or unknown inputs broaden selection. | Integrate the pilot, measure full versus affected runs, then prepare a Quality policy around the existing package and native graph. Release/manual full qualification remains. |
| website `release/v5.3.0` | Cache archive site builds with runtime, calendar, timezone, and Git-history inputs; keep the current site fresh. Persist Turbo cache in existing workflows. | Integrate and measure cold/warm CI; prepare policy for archives, current site, assembly, browser and performance consumers. Retain production validation. |
| PostLens `release/v2.1.0` | One website build artifact feeds browser/visual checks and deployment; verify source, run, artifact identity, and content digest. Aggregate gate validates selected results. | Integrate and prove retry reuse in CI; prepare policy for website, marketing, Android and Apple boundaries. Apple qualification stays separate. |

Quality currently **plans and verifies**; it does not include a general execution
adapter that turns plan batches into platform jobs and collects results. The
pilots use repository-owned workflow logic. Quality's shadow plan does not skip
the canonical full gate. Cache reuse and artifact sharing do not independently
authorize selective release validation.

## Remaining repository inventory

Every row below remains pending for this rollout. The release column lists
locally observed `release/` refs, **not a decision that the highest version is
active**. Confirm the intended integration branch and its worktree owner before
editing. A dash means no local release ref was observed; it does not mean a new
version should be invented. Existing historical refs are retained in this table
to make ambiguity visible.

| Repository | Observed release refs (under `release/`) | Concrete policy and workflow boundary to prepare |
| --- | --- | --- |
| aaronmgz | — | Separate site build/lint/test from era captures; preserve `validate.yml` and Cloudflare `deploy.yml`. |
| astro-doctor | `v1.5.0`, `v1.6.0`, `v1.7.0` | CLI diagnostics, compatibility fixtures, extension and docs nodes; retain Node/Windows compatibility, `publish.yml`, and Cloudflare docs deployment. |
| auth | `v0.4.0`, `v0.5.1` | Package/adapter tests and website consumers; preserve `verify`, security audit, package release and website workflows. Authentication policy and migrations require explicit shared inputs. |
| commitprompt | `local-commitprompt-candidate`, `v1.1.1`, `v1.2.0` | CLI/package tests and docs build; retain `validate`, Changesets release and Cloudflare docs workflows. |
| ContracTrack | `v1.0.18`, `v1.1.1`, `v1.2.0` | JavaScript, Android, API and website nodes; preserve separate iOS/macOS launch/monitor workflows, Android delivery and Cloudflare targets. Current checkout is `release/v1.0.18`; resolve the intended newer release explicitly. |
| coolstead | `v1.2.1` | Portable checks and website nodes; keep Xcode Cloud qualification separate from the GitHub release/Cloudflare website path. |
| cult | `v0.3.0`, `v2.1.0` | Build/check/test/verify and web deployment nodes; keep audit, Lighthouse and Astro diagnostics as explicit consumers. Current checkout is an archive branch: choose the release worktree first. |
| dep-beacon | `v1.3.0`, `v1.4.0` | CLI/package tests, docs and editor extension nodes; preserve npm/GitHub release and separate Zed delivery. Advisory intelligence needs fresh security checks. |
| eslint-config-basic | `v3.6.2` | Config packages, representative playgrounds and docs; preserve package-dependent consumer tests, Changesets preparation, npm publication and release recovery. |
| extensions | — | Extension package/build tests and manifest validation; preserve existing CI and review checks. This checkout is a separate extension registry, not another app deployment. |
| fenix | `v1.1.2`, `v1.2.0`, `v1.5.0` | App/package consumers, web and Cartera targets; reuse existing exact-tree CI evidence rules, with separate target delivery baselines. Preserve `verify` and both deployment workflows. |
| homebrew-tap | — | Formula/source checks and upstream artifact identities; use a small policy without Turbo. No application build or deployment workflow was observed. |
| kinjar | `v0.1.1` | API, mobile and web checks plus `release:preflight`; retain dependency security and deployment-readiness packaging. Readiness bundles are not store or production delivery. |
| memudo.ai | `v0.0.1` | Site build/check/lint/typecheck and Cloudflare deployment; classify shared content/configuration conservatively. |
| observatory | `v0.6.0` | App/server/website checks, `verify`, and fresh security audit; preserve Cloudflare deployment and release workflow boundaries. |
| roadscore | `v1.4.0`, `v1.5.0`, `v2.0.0` | Mobile, API and website inputs plus shared contracts; retain `release:preflight`, deployment readiness, mobile build/submission and Cloudflare website workflows. Verify the configured mobile provider before changing execution. |
| santi020k | — | Profile/content checks and generated assets; preserve profile, blog and scheduled snake workflows. No Turbo or application build is required. |
| santi020k-og | `v1.2.2` | Generator package, output fixtures and website consumers; retain `validate`, site inspection, npm/GitHub release and Cloudflare deployment. |
| santi020k-theme | `v2.2.0`, `v3.0.0`, `v3.1.0` | Theme sources, generated packages, websites and consumer fixtures; keep Chrome, terminal and Zed release adapters separate. |
| santi020k-way | `v0.1.0`, `v1.0.0` | Package/site build, tests and `verify`; retain check, website deployment and release workflows. |
| til | — | Content/link validation policy only where useful. No workflow or package task gate was observed; establish a minimal real check before adding an aggregate success gate. |
| workspace-organizer | `local-workscene-assessment-20261008`, `v0.3.0` | Extend existing API/native/website scope classification; retain the explicit native-provider gate, Xcode Cloud boundary, and separate Cloudflare API/website deployments. Current checkout is `release/v0.3.0`. |

Komori requires an inventory repair before adoption: the top-level
`komori-home-game`, `komori-social-release`, and `komori-watch-validation`
directories have Git pointers that do not resolve in this environment. Treat
them as one project, not three independent rollout targets. The available
manifests expose `check`, `test`, and `verify`; social and release workflows are
present. Recover the authoritative checkout and release ref before preparing
its native/social-service policy. Do not infer a release version from README
copy or modify these preserved checkouts.

## Preparation and activation checklist

- [ ] For each pending repository, confirm its intended release and ownership;
  record current full-gate timing and bottlenecks.
- [ ] Prepare source-only `quality-impact.json` and fixtures locally now. Map
  shared inputs, task prerequisites, downstream fixtures, native adapters and
  deployment targets. This does **not** require waiting for publication.
- [ ] Implement and test the trusted execution adapter: consume dependency
  batches, bound parallelism, collect exact per-node exit status, and invoke
  `quality impact verify` from an always-running aggregate job. Do not execute
  contributor-supplied plans with release credentials.
- [ ] After Quality 2 publication and verification, pin the binary/Action and
  reusable workflows to verified immutable identities. Do not reference an
  unpublished consumer version.
- [ ] Run shadow plans beside the existing full gate. Exercise unknown paths,
  lockfiles, generators, renames, deletions, shared configuration, dependency
  consumers and missing history before approving selective required checks.
- [ ] Share immutable artifacts from the exact qualified revision; record a
  separate last-successfully-delivered revision for every deployment target.
  Failed delivery must not advance its baseline. Retry failed consumers while
  the verified artifact remains available; rebuild when it expires.
- [ ] Activate selective gates only after reviewing measured coverage. Keep a
  full-selection recovery switch and periodic full qualification. Report each
  repository's local integration, CI proof and production result separately.

Use Turbo for workspace task scheduling and caching, not as an Apple or store
delivery provider. Public Apple repositories use standard GitHub-hosted macOS
runners; private Apple repositories use Xcode Cloud. Verify actual repository
visibility before selecting or changing a provider. GitHub Actions orchestrates
portable checks and releases in both cases. Existing store review, signing,
protected environments, migration checks and deployment approvals remain
required. Parallelize independent work, but serialize shared output directories
and deployment writers to the same target.

The version/changelog layer continues to use reviewed Changesets and each
repository's release adapters. Adopting the owned renderer can be prepared with
local fixtures; it must not replace dependency/version propagation or infer
release impact from changelog fragments alone.
