import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { deploymentEligibility, githubApi } from "./deployment-eligibility.mjs";

const sha = "a".repeat(40);
const tagSha = "b".repeat(40);
const repository = "fixture/quality";
const context = { repository, sha, version: "2.0.0" };
const mainRef = encodeURIComponent("refs/heads/main");
const versionRef = encodeURIComponent("refs/tags/v2.0.0");
const verifiedRun = { id: 12, head_sha: tagSha, head_branch: "main", head_repository: { full_name: repository }, event: "push", conclusion: "success" };

function fixture(overrides = {}) {
  const responses = {
    [`/repos/${repository}/commits/${mainRef}`]: { sha },
    [`/repos/${repository}/releases/tags/v2.0.0`]: { tag_name: "v2.0.0", draft: false, prerelease: false },
    [`/repos/${repository}/commits/${versionRef}`]: { sha: tagSha },
    [`/repos/${repository}/compare/${tagSha}...${sha}`]: { status: "ahead" },
    [`/repos/${repository}/actions/workflows/release.yml/runs?head_sha=${tagSha}&status=success&per_page=100`]: { workflow_runs: [verifiedRun] },
    [`/repos/${repository}/actions/runs/12/jobs?filter=latest&per_page=100`]: { jobs: [
      { name: "Verify published Action", conclusion: "success" },
      { name: "Promote verified stable release", conclusion: "success" },
    ] },
    ...overrides,
  };
  return async (path) => {
    assert.ok(Object.hasOwn(responses, path), `Unexpected API request: ${path}`);
    return responses[path];
  };
}

test("documentation after a verified release uses its exact main source SHA", async () => {
  const result = await deploymentEligibility(context, fixture());
  assert.equal(result.eligible, true);
  assert.equal(result.sha, sha);
  assert.equal(result.tag, "v2.0.0");
});

test("unpublished versions and stale source revisions wait without deploying", async () => {
  for (const override of [
    { [`/repos/${repository}/releases/tags/v2.0.0`]: null },
    { [`/repos/${repository}/commits/${mainRef}`]: { sha: "c".repeat(40) } },
  ]) {
    assert.equal((await deploymentEligibility(context, fixture(override))).eligible, false);
  }
});

test("a successful workflow with skipped smoke or promotion does not qualify a release", async () => {
  const key = `/repos/${repository}/actions/runs/12/jobs?filter=latest&per_page=100`;
  for (const conclusion of ["skipped", "failure", "cancelled"]) {
    assert.equal((await deploymentEligibility(context, fixture({ [key]: { jobs: [
      { name: "Verify published Action", conclusion },
      { name: "Promote verified stable release", conclusion: "success" },
    ] } }))).eligible, false);
  }
});

test("draft and prerelease versions never replace production documentation", async () => {
  assert.equal((await deploymentEligibility({ ...context, version: "2.0.0-rc.1" }, fixture())).eligible, false);
  for (const release of [
    { tag_name: "v2.0.0", draft: true, prerelease: false },
    { tag_name: "v2.0.0", draft: false, prerelease: true },
    { tag_name: "v3.0.0", draft: false, prerelease: false },
  ]) {
    assert.equal((await deploymentEligibility(context, fixture({ [`/repos/${repository}/releases/tags/v2.0.0`]: release }))).eligible, false);
  }
});

test("a tag outside the source history fails closed", async () => {
  await assert.rejects(deploymentEligibility(context, fixture({
    [`/repos/${repository}/compare/${tagSha}...${sha}`]: { status: "diverged" },
  })), /not an ancestor/);
});

test("unverified, failed, fork, branch and wrong-commit workflow results cannot qualify a release", async () => {
  const key = `/repos/${repository}/actions/workflows/release.yml/runs?head_sha=${tagSha}&status=success&per_page=100`;
  for (const runs of [
    [], [{ ...verifiedRun, conclusion: "failure" }], [{ ...verifiedRun, event: "pull_request" }],
    [{ ...verifiedRun, head_sha: sha }], [{ ...verifiedRun, head_branch: "release/v2.0.0" }],
    [{ ...verifiedRun, head_repository: { full_name: "fork/quality" } }],
  ]) {
    assert.equal((await deploymentEligibility(context, fixture({ [key]: { workflow_runs: runs } }))).eligible, false);
  }
});

test("API errors fail explicitly without echoing response bodies or credentials", async () => {
  const notFound = await githubApi("/missing", { token: "fixture-token", request: async () => new Response("", { status: 404 }) });
  assert.equal(notFound, null);
  await assert.rejects(githubApi("/forbidden", {
    token: "fixture-token", request: async () => new Response("private response", { status: 403 }),
  }), /^Error: GitHub deployment verification failed \(HTTP 403\)\.$/);
});

test("deployment checks, artifacts, and secrets remain bound to an eligible main commit", () => {
  const workflow = readFileSync(".github/workflows/deploy-site.yml", "utf8");
  assert.match(workflow, /workflow_run:\n    workflows: \[Release\]\n    types: \[completed\]\n    branches: \[main\]/);
  assert.match(workflow, /github.event.workflow_run.conclusion == 'success'/);
  assert.match(workflow, /github.event.workflow_run.head_repository.full_name == github.repository/);
  assert.match(workflow, /actions: read/);
  assert.equal(workflow.match(/ref: \$\{\{ needs.eligibility.outputs.sha \}\}/g)?.length, 3);
  assert.equal(workflow.match(/if: steps.current.outputs.ready == 'true'/g)?.length, 2);
  assert.match(workflow, /group: deploy-site-production\n  cancel-in-progress: false/);
});

test("every main push queues a replacement when a newer source commit supersedes pending docs", () => {
  const workflow = readFileSync(".github/workflows/deploy-site.yml", "utf8");
  const push = workflow.match(/^  push:\n(?<settings>(?:^    .*\n)+)/m)?.groups?.settings;
  assert.ok(push, "deployment must listen for main pushes");
  assert.match(push, /^    branches: \[main\]$/m);
  assert.doesNotMatch(push, /^    (?:paths|paths-ignore):/m,
    "README or Rust updates must queue the current main SHA after superseding a documentation build");
});
