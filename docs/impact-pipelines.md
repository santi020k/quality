# Impact pipelines

Quality 2 separates validation planning from execution and delivery. Turborepo
continues to schedule and cache workspace tasks; native tools keep their own
build caches. Quality does not publish packages or deploy applications.

## Plan a committed revision

Declare `quality-impact.json` at the repository root. The repository's own file
is a conservative example, initially used alongside the canonical full gate.
Existing `quality.yml` check tasks and `quality ci plan` are unchanged.

```sh
quality impact plan --base origin/main --head HEAD --comparison merge-base --format json > plan.json
quality impact plan --full --format json > full-plan.json
```

PR validation compares the merge base with the selected head. For deployment
planning, use `--comparison direct --base LAST_DELIVERED_SHA --head HEAD` and
resolve the baseline separately for each target. A failed deployment must not
advance that target's baseline. Plans inspect committed inputs; uncommitted
edits are not evidence for a CI revision. Use `quality check --changed` for
local source checks.

The configuration has `schema_version: 1`, `global_inputs`, `ignored_inputs`,
and a map of `nodes`. Each node declares `inputs`, `depends_on`, `always`,
`command`, `args`, and an optional `working_directory`. Inputs are exact paths
or directory prefixes ending in `/`; they are not glob patterns.

A changed input selects its owning node and downstream consumers. The plan also
includes prerequisite nodes and deterministic execution batches. Shared inputs,
unclassified files, or unavailable Git comparison data select the complete
configured graph. Malformed configuration and dependency cycles fail planning.
Keep the configuration and planner implementation in global inputs. Ignoring a
path is an explicit assertion that it cannot affect any configured check.

## Execute and verify

Planning never executes commands. A trusted workflow consumes selected nodes
and their dependency batches, runs the declared commands, and records results.
Use Turbo for workspace parallelism and separate jobs for different platforms.
Never execute a plan downloaded from an untrusted contributor with credentials.

```json
{
  "schema_version": 1,
  "plan_id": "identity from the plan",
  "head": "resolved source revision from the plan",
  "results": [
    {"id": "site-build", "status": "passed", "exit_code": 0}
  ]
}
```

```sh
quality impact verify --plan plan.json --results results.json --format json
```

Verification requires a clean checkout with the policy committed at the planned
revision. Only the exact untracked plan and results files supplied to the verifier
are exempt; ignored build artifacts are allowed. Planning can preview a local
policy, but such a policy cannot satisfy verification until committed.

Supply exactly one successful result per selected node. Missing, duplicate,
unknown, skipped, failed, or cancelled results must not satisfy the gate. The
identity (`git-blob:<digest>`, calculated with Git) binds the plan and revision;
it does not authenticate whoever supplied
the results. Keep plan generation, results, and artifact provenance within a
trusted workflow. This initial contract does not authorize cross-run reuse of
arbitrary evidence. Turbo may reuse its valid task cache during execution.

## Cache and artifact boundaries

Cache identities must include relevant source and transitive inputs, task
configuration, dependency resolution, toolchain, platform, and output-affecting
environment. Live advisory checks, production state, and migrations need fresh
verification. Website clock and network inputs must be explicit snapshots or
freshness inputs before caching their output.

Build release artifacts from the merged main revision. Share the same output
between validation and delivery and verify its source revision and digest.
Retry failed jobs against that artifact; do not rebuild after every provider
failure. Retain separate state for validation, artifact identity, and actual
delivery. Serialize writers to a shared output directory and deployments to the
same production target.

## Adoption and recovery

See the [cross-project rollout inventory](impact-rollout.md) for implemented
pilots, observed release refs, and repository-specific adoption work remaining.

Keep the complete release gate while comparing impact plans with real full
runs. Test shared configuration, generators, deletions, renames, lockfiles,
unknown paths, and transitive consumers before changing required selection.
An always-running final gate must verify every selected result. A periodic full
run supplements, but does not replace, affected release qualification.

Start with the existing repository-local affected pilot and this committed
plan. Use the pinned published Quality 2 binary only after its normal release
workflow succeeds; do not update consumers to an unpublished version.
Restore full selection with `quality impact plan --full`. The canonical
`pnpm validate` and production browser gate remain the recovery path. There is
no persisted application-data migration.

## Release notes

Reviewed Changesets remain the source of version intent and migration notes.
The owned `scripts/changelog.mjs` renderer formats those notes deterministically
without GitHub enrichment or network access. Changesets remains responsible for
version propagation. Native manifest adapters remain repository-owned. A change
fragment never substitutes for source-based impact analysis.
