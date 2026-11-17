# Impact rollout across sibling projects

Inventory refreshed on 2026-11-17 against top-level sibling manifests, local Git
refs, and `.github/workflows/`. Quality's verified publication and deployment
are recorded below; they do not imply completion across all repositories.
Backup directories and duplicate scratch checkouts are excluded.

## Implemented pilots

These are the selected release targets for this work. Integration, publication,
and production verification must be reported separately from implementation.
Reuse each project's unfinished working release. When its previous release has
already shipped, prepare the next minor release; website therefore targets
`release/v5.4.0` after the published `5.3.0`.

| Project and selected release | Implemented change | Remaining adoption |
| --- | --- | --- |
| Quality `v2.0.0` (published and deployed) | Deterministic impact plan and result verification, repository policy, shadow CI plan, owned Changesets renderer. Release and production verification completed. Existing full checks remain required. | Consumers now pin the verified revision locally; hosted policy qualification and consumer releases remain pending. |
| Lumen `release/v5.3.0` | Quality v2 shadow policy, selective Apple checks, resolved-runtime cache identity, and docs deployment retries using an exact artifact ID and trusted checksum. Shared or unknown inputs broaden selection. | Locally integrated; measure hosted runs. Release/manual full qualification remains. |
| website `release/v5.4.0` | Cache archive site builds with runtime, calendar, timezone, and Git-history inputs; keep the current site fresh. Persist Turbo cache in existing workflows. | Locally integrated; measure cold/warm CI; qualify the shadow policy for archives, current site, assembly, browser and performance consumers. Retain production validation. |
| PostLens `release/v2.1.0` | Quality v2 shadow policy and one website build artifact feeding browser/visual checks and deployment; verify source, run, artifact identity, and content digest. Aggregate gate validates selected results. | Locally integrated; prove retry reuse in CI. Policy fixtures cover website, shared inputs and missing history; Apple qualification stays separate. |

Quality v2.0.0 was published and deployed on 2026-11-16 (America/Bogota):

- [PR #64](https://github.com/santi020k/quality/pull/64) merged as
  `ff490e9efbd5fdfbdb1e6c404f1741d04c57f585`. Both `v2.0.0` and `v2` resolved
  to that commit at verification; use the full commit for immutable workflow pins.
- The [release workflow](https://github.com/santi020k/quality/actions/runs/43992831909)
  published [v2.0.0](https://github.com/santi020k/quality/releases/tag/v2.0.0).
  All five native archives passed checksum and signed provenance verification;
  the published Action and installer smoke checks passed.
- The [deployment workflow](https://github.com/santi020k/quality/actions/runs/43993984409)
  deployed [the documentation site](https://quality.santi020k.com).
  All 13 live schemas matched the merged revision. Production browser checks
  passed 64 tests with two expected skips using a temporary harness that awaited
  the asynchronous theme toggle before testing navigation. The unmodified suite
  exposed a timing race in that test; the deployed theme persistence was verified.

Quality currently **plans and verifies**; it does not include a general execution
adapter that turns plan batches into platform jobs and collects results. The
pilots use repository-owned workflow logic. Quality's shadow plan does not skip
the canonical full gate. Cache reuse and artifact sharing do not independently
authorize selective release validation.

## Consumer implementation status

Local rollout status on 2026-11-17: all 26 consumer implementations are committed;
20 are integrated into their selected local release branches. Four unversioned
repositories retain focused branches for their normal main pull request. ESLint
Config integration waits for its active release worktree; Komori waits for its
mandatory native verification. No consumer rollout has been pushed or deployed.
Original working changes remain preserved. A local integration is not a release
qualification or hosted performance result.

| Project | Selected target | Commit | Local integration |
| --- | --- | --- | --- |
| ContracTrack | `release/v1.2.0` | `05af5ac7d80a` | Integrated |
| aaronmgz | `release/v3.1.0` | `74dd7d25f733` | Integrated |
| astro-doctor | `release/v1.8.0` | `c786a19de32a` | Integrated |
| auth | `release/v0.6.0` | `5c2d0b8815f1` | Integrated |
| commitprompt | `release/v1.2.0` | `02fffa35c253` | Integrated |
| coolstead | `release/v1.2.1` | `09801fcb7f49` | Integrated |
| cult | `release/v2.4.0` | `eec6b082a462` | Integrated |
| dep-beacon | `release/v1.5.0` | `837a60134e10` | Integrated |
| eslint-config-basic | `release/v3.8.0` | `9c4ea6d0ca65` | Awaiting idle release worktree |
| extensions | `main (unversioned)` | `078eae95d6fb` | Focused branch; main PR pending |
| fenix | `release/v1.5.0` | `f99d9eb25c7a` | Integrated |
| homebrew-tap | `main (unversioned)` | `0d43489bd69c` | Focused branch; main PR pending |
| kinjar | `release/v0.1.1` | `5adaade0ec02` | Integrated |
| lumen | `release/v5.3.0` | `8d4cecdce0a3` | Integrated |
| memudo.ai | `release/v0.0.1` | `3f1cec2615b3` | Integrated |
| observatory | `release/v0.6.0` | `a4dc490f38de` | Integrated |
| PostLens | `release/v2.1.0` | `d2750f39f8ab` | Integrated |
| roadscore | `release/v1.4.0` | `b0d28ac92e28` | Integrated |
| santi020k | `main (unversioned)` | `a3d501d83352` | Focused branch; main PR pending |
| santi020k-og | `release/v1.2.2` | `f857921989b8` | Integrated |
| santi020k-theme | `release/v3.2.0` | `beec9ae43fb6` | Integrated |
| santi020k-way | `release/v1.0.0` | `bbd59e5e290d` | Integrated |
| til | `main (unversioned)` | `e3fd661c1e48` | Focused branch; main PR pending |
| website | `release/v5.4.0` | `c6172a999f74` | Integrated |
| workspace-organizer | `release/v0.3.0` | `a546bbadc8f9` | Integrated |
| komori | `release/v0.5.0` | `326e3adb2607` | Required native gate blocked |

### Verification and remaining gates

- Every consumer has a real Quality v2 CLI plan verified against committed source,
  policy fixtures, and focused project checks. Existing full/security/native gates
  remain required. Unknown or missing history broadens selection.
- Website passed its production build and link audit (2,231 documents and 7,606
  targets). PostLens production and preview builds verified the different store
  availability settings and reusable artifact digest. Production builds retain
  their protected environment and configuration before building.
- Astro Doctor passed 593 package tests, three dependency-security tests, canonical
  lint, and seven typecheck tasks. Coolstead and Workscene passed canonical native
  type checks with their exact locked Swift packages unchanged.
- Auth's full `pnpm check` remains blocked by an existing packed-consumer npm
  dependency-resolution loop involving `better-call` 1.4.4 and 1.4.5.
- Observatory's `pnpm verify` retains 11 dependency advisories, including critical
  and high Better Auth findings. Cult retains four existing dependency advisories.
  Their audits must pass before release; selective checks do not hide failures.
- Komori passed portable, Godot, iOS/watch build and bundle checks, but required
  `scripts/verify.sh` cannot finish its native adapter tests: Xcode requires
  simulator build `24B8118`; the installed runtime differs and Apple's download
  service reports that exact build unavailable. Integration remains blocked.
- RoadScore's existing private Apple/EAS provider mismatch needs a separate Xcode
  Cloud migration. KinJar has no remote/deployment provider; its validated dry run
  demonstrates readiness packaging, not production delivery.
- Lumen passed its complete `pnpm run validate`: 3,807 tests passed with one
  existing skip, plus builds, types, lint, security and packaged-consumer checks.
  OG passed full `pnpm run validate` with the preserved Lumen 5 upgrade. MeMudo
  passed forced lint/type checks across all 11 workspaces and nine CI tests after
  preserving that upgrade. Hosted cold/warm timing, retry recovery, and production
  smoke checks remain outstanding across consumers.

Persisted Turbo outputs include the resolved Node/pnpm runtime, platform and
architecture in task identity. The shared setup cache key alone identifies the
declared Node selector, which is insufficient to distinguish patch changes.
Runtime invalidation was verified against real task graphs. MeMudo, Aaronmgz and
Observatory also propagate dependency source hashes through virtual tasks, with
real Turbo regression tests for consumer invalidation. Fresh security and
clock-sensitive outputs remain outside inappropriate cache reuse.

## Remaining repository inventory

The inventory below records the rollout scope; local implementation progress is
listed above. Remaining integrations and hosted qualification remain pending. This is a
pre-rollout inventory snapshot. The release column lists
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

Komori was recovered into an authoritative isolated checkout and prepared for
`release/v0.5.0`. Its original broken scratch checkouts (`komori-home-game`,
`komori-social-release`, and `komori-watch-validation`) remain untouched. It is
one consumer project. The native verification blocker is recorded above.

## Preparation and activation checklist

- [x] Publish Quality v2.0.0 through GitHub Actions and verify the native
  archives, published Action, installer, and release identity.
- [x] Deploy Quality's documentation and verify live schemas and desktop/mobile
  behavior against the merged release revision.
- [ ] For each pending repository, confirm its intended release and ownership;
  record current full-gate timing and bottlenecks.
- [x] Prepare source-only `quality-impact.json` and fixtures locally. Map
  shared inputs, task prerequisites, downstream fixtures, native adapters and
  deployment targets. This does **not** require waiting for publication.
- [ ] Implement and test the trusted execution adapter: consume dependency
  batches, bound parallelism, collect exact per-node exit status, and invoke
  `quality impact verify` from an always-running aggregate job. Do not execute
  contributor-supplied plans with release credentials.
- [x] Pin consumer binaries to published Quality v2.0.0 and the Action/reusable
  workflows to verified immutable identities. Publication is complete;
  local adoption is committed; hosted validation remains pending.
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
