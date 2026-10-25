# Affected validation pilot

This is a repository-local pilot, not a new public Quality CLI contract. It
uses the existing Quality analyzers, Turbo package graph and cache, and native
test runners. No dependencies or services are added.

## Commands and selection

Run `pnpm ci:plan --base HEAD` to preview local edits, or use the integration
branch (for example `origin/main`) to include committed branch changes. Run
`pnpm run ci --base HEAD` to execute the same selection. `--report /tmp/quality-ci.json`
writes selected tasks, skip reasons, timings and exit codes. `--full` explicitly
runs `pnpm validate`.

The base is resolved to a commit and compared from its merge base with the
current checkout. Staged, unstaged, untracked, deleted and both sides of renamed
paths participate. CI supplies the PR base SHA and checks the checkout's merged
revision. An inherited Turbo head cannot exclude local edits.

| Changed input | Validation |
| --- | --- |
| Root README or documentation assets | Changed analyzers, live audit, repository script and release-reference checks |
| Action workspace | Changed analyzers, affected workspace tests and builds, committed Action bundle verification |
| Site workspace | Changed analyzers, affected tests and builds, complete production browser suite |
| Rust workspace | Full analyzer check, affected tests and builds, playground verification |
| Playground | Changed analyzers and playground verification |
| Shared configuration, lockfiles, templates, report schemas, root scripts, unknown paths | Full validation and production browser suite |
| Missing Git base or unavailable Turbo graph | Full validation and production browser suite |

Every affected run includes the live dependency audit and repository script /
release-reference checks. Turbo selects transitive dependents, and every test
in a selected package runs. The planner checks that directly changed packages
were not omitted. A malformed or incomplete graph broadens validation.

GitHub's documentation job uses the same planner and keeps its existing check
name. It skips browser installation, build and browser execution when the site
is unaffected. It still runs its checkout, dependency setup and plan, so a
planning failure fails the job. The documentation and main validation jobs stay
parallel. The main job uploads a timing report even when a check fails.

The Playwright preview command uses Astro's `--ignore-lock` foreground mode.
This prevents automatic background startup in agent environments from leaving
an orphan preview server after a benchmark or test run.

Manual workflow runs and release branches use full validation. The full
`validate` script, pre-push hook and Release workflow are retained. Browser
verification remains a separate job / `pnpm test:site` command; the local
`pnpm run ci` report indicates when it is required but does not launch a browser.

Ordinary pull requests may target `main` or an active `release/v*` branch. Both
change selection and the Changeset check use the actual PR base commit. This
allows a normal feature PR to exercise affected mode before the release is
merged, while a PR whose head is a release branch still runs full validation.

Use the explicit `run` here: pnpm 11's built-in `pnpm ci` performs a clean
dependency installation rather than running the repository's validation script.

Turbo hashes root Cargo inputs, CLI templates and shared report schemas.
Native CLI builds use Cargo's own incremental cache rather than caching a
successful Turbo task without its binary. Browser tests and live audits are not
cached. No timing improvement changes the release requirements.

## Reproducing the benchmark

```sh
pnpm benchmark:ci --output /tmp/quality-ci-benchmark --repetitions 2
```

An optional `--scenario` selects `readme`, `action-test`, `site-content`,
`rust-source`, or `shared-config`. The script copies current non-ignored source
into a temporary Git repository, installs the locked dependencies offline, and
applies a harmless comment edit in each scenario. The original checkout's
source and Git state are not changed. Cargo's existing target directory is
reused as a compiler cache. Temporary sources are removed at the end.

Each scenario warms the compiler first, then compares full and affected
validation with separate empty Turbo caches and repeated warm caches. Execution
order alternates. Remote task caching is disabled. "Cold" means an empty Turbo
task cache, not an empty dependency store or compiler cache. The JSON report
records each command's exit code, wall time and Turbo cache summary; raw logs
are retained alongside it. Planning time is recorded separately.

The timed full workload is derived from the actual `validate` script. The live
network audit is measured separately, as is the production browser suite. Both
remain mandatory in the real workflow, and the benchmark exits unsuccessfully
if either fails. Local wall times do not predict hosted runner setup, queueing
or parallel-job duration. Use successful CI reports before expanding the pilot.

## Pilot measurements — 2026-10-25

Five isolated edits were measured with one cold task-cache run and two warm
runs per mode (30 measured workloads, plus compiler warmups). The following
times include the affected planner's measured overhead; warm times are the
mean of the two runs. They cover deterministic validation, excluding the live
audit and separate browser job.

Environment: Apple M5 Pro, macOS arm64, Node 22.23.1, pnpm 11.22.0 and Turbo
2.11.7. Dependencies were installed from the existing offline store.

| Edit | Full, cold | Affected, cold | Full, warm | Affected, warm | Warm reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Root README | 16.19 s | 2.74 s | 12.59 s | 2.63 s | 79% |
| Action test | 16.44 s | 6.54 s | 12.57 s | 4.84 s | 61% |
| Site content | 16.11 s | 4.40 s | 12.51 s | 2.90 s | 77% |
| Rust source | 16.65 s | 12.58 s | 12.32 s | 10.34 s | 16% |
| Shared configuration | 17.72 s | 16.43 s | 12.38 s | 12.78 s | Full fallback |

All measured deterministic commands passed. The shared-configuration row runs
the same full workload in both modes; its small timing difference is run
variation, not reduced coverage. Site-content changes still require the full
browser suite. That suite passed all 63 checks in 16.87 seconds wall time when
measured separately. Unrelated edits skip that browser workload in CI, while
retaining the documentation job's setup and planning costs.

The live audit took 0.75 seconds and failed on 16 existing dependency advisories
(5 high, 9 moderate, 2 low), also observed before the pilot. That benchmark
exited with failure for the audit; these timings do not establish release
readiness. A subsequent [dependency security follow-up](../RELEASING.md#dependency-security-follow-up--2026-10-25)
patched those dependencies, and a fresh `pnpm check:security` reported no known
vulnerabilities. Both affected and full validation retain the live audit gate.
The original timing samples were not rerun or rewritten after remediation.

Measurements used the working source snapshot recorded in `results.json`;
Git-hook isolation and its regression test were hardened afterward without
changing task selection. Reproduce the benchmark on CI before treating these
local timings as an expected hosted-runner improvement.

## Rollback

Run `pnpm validate` and `pnpm test:site` at any time. To disable affected CI,
set `QUALITY_CI_FULL=true` for both validation jobs. No release flag, package
version, public interface or persisted user data needs migration.
