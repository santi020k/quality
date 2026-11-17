import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { activeReleaseBranches, checkConsumedChangesets } from "./check-release-preparation.mjs";

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "quality-release-preparation-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "--quiet");
  git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "Initial");
  const base = git("rev-parse", "HEAD");
  git("update-ref", "refs/remotes/origin/main", base);
  return { root, git, base };
}

test("release PRs reject all remaining change fragments and allow consumed metadata", async (t) => {
  const f = fixture(t);
  mkdirSync(join(f.root, ".changeset"));
  writeFileSync(join(f.root, ".changeset/README.md"), "Changeset instructions");
  writeFileSync(join(f.root, ".changeset/config.json"), "{}");
  await checkConsumedChangesets(f.root);
  writeFileSync(join(f.root, ".changeset/pending.md"), "---\n---\n");
  await assert.rejects(checkConsumedChangesets(f.root), /must consume all Changesets.*pending.md/);
});

test("a deliberate release branch blocks automation before its first unique commit", (t) => {
  const f = fixture(t);
  f.git("update-ref", "refs/remotes/origin/release/v2.0.0", f.base);
  assert.deepEqual(activeReleaseBranches(f.root), ["release/v2.0.0"]);
});

test("completed historical release branches do not block new version preparation", (t) => {
  const f = fixture(t);
  f.git("update-ref", "refs/remotes/origin/release/v1.0.0", f.base);
  f.git("tag", "v1.0.0", f.base);
  assert.deepEqual(activeReleaseBranches(f.root), []);
  f.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "New release work");
  f.git("update-ref", "refs/remotes/origin/release/v1.0.0", f.git("rev-parse", "HEAD"));
  assert.deepEqual(activeReleaseBranches(f.root), ["release/v1.0.0"]);
});

test("version preparation uses scoped API authentication without install scripts", () => {
  const workflow = readFileSync(".github/workflows/release-pr.yml", "utf8");
  assert.match(workflow, /if: github.ref == 'refs\/heads\/main'/);
  assert.match(workflow, /persist-credentials: false/);
  assert.match(workflow, /pnpm install --frozen-lockfile --ignore-scripts/);
  assert.match(workflow, /github-token: \$\{\{ secrets.CHANGESETS_TOKEN \|\| github.token \}\}/);
  assert.doesNotMatch(workflow, /push-with-git-cli: true|GITHUB_TOKEN:/);
  assert.match(workflow, /npm_config_ignore_scripts: "true"/);
  assert.match(readFileSync(".github/workflows/ci.yml", "utf8"), /node scripts\/check-release-preparation.mjs consumed/);
});
