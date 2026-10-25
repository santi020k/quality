import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, renameSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import { changedFiles, classify, createPlan, gitEnvironment, runPlan, turboPackages } from "./affected-ci.mjs";

const repository = resolve(import.meta.dirname, "..");

function fixture(t, { dependent = false } = {}) {
  const cwd = mkdtempSync(join(tmpdir(), "quality-affected-"));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  const git = (...args) => {
    const result = spawnSync("git", args, { cwd, encoding: "utf8", env: gitEnvironment() });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  const write = (path, content = "export {};\n") => {
    mkdirSync(resolve(cwd, path, ".."), { recursive: true });
    writeFileSync(join(cwd, path), content);
  };
  git("init", "-q");
  git("config", "user.name", "Quality tests");
  git("config", "user.email", "tests@example.invalid");
  write("package.json", JSON.stringify({ name: "fixture", private: true, packageManager: "pnpm@11.22.0" }));
  write(".gitignore", "node_modules\n.turbo/\n");
  write("pnpm-workspace.yaml", "packages:\n  - apps/*\n  - crates/*\n  - packages/*\n");
  write("pnpm-lock.yaml", readFileSync(join(repository, "pnpm-lock.yaml"), "utf8"));
  write("turbo.json", JSON.stringify({ tasks: { build: { dependsOn: ["^build"] }, test: {} } }));
  for (const [path, name] of [["apps/site", "@quality/site"], ["crates/quality-cli", "@quality/cli"], ["packages/action", "@quality/action"]]) {
    write(`${path}/package.json`, JSON.stringify({ name, scripts: { build: "echo build", test: "echo test" },
      ...(dependent && name === "@quality/site" ? { dependencies: { "@quality/action": "workspace:*" } } : {}) }));
    write(`${path}/src/source.ts`);
  }
  write("README.md", "# Fixture\n");
  git("add", ".");
  git("commit", "-qm", "Fixture");
  symlinkSync(join(repository, "node_modules"), join(cwd, "node_modules"), "dir");
  return { cwd, git, write, base: git("rev-parse", "HEAD") };
}

test("includes committed, staged, unstaged, untracked, deleted and renamed paths", (t) => {
  const f = fixture(t);
  f.write("README.md", "# Updated\n");
  f.git("add", "README.md");
  f.git("commit", "-qm", "Documentation");
  f.write("packages/action/src/source.ts", "export const value = 1;\n");
  f.git("add", "packages/action/src/source.ts");
  f.write("apps/site/src/new file.ts");
  renameSync(join(f.cwd, "apps/site/src/source.ts"), join(f.cwd, "apps/site/src/renamed.ts"));
  rmSync(join(f.cwd, "crates/quality-cli/src/source.ts"));
  assert.deepEqual(changedFiles(f.cwd, f.base).files, [
    "README.md", "apps/site/src/new file.ts", "apps/site/src/renamed.ts", "apps/site/src/source.ts",
    "crates/quality-cli/src/source.ts", "packages/action/src/source.ts",
  ]);
});

test("compares from the merge base without counting target-only changes", (t) => {
  const f = fixture(t);
  f.git("checkout", "-qb", "target");
  f.write("target-only.txt", "target\n");
  f.git("add", "."); f.git("commit", "-qm", "Target change");
  f.git("checkout", "-qb", "feature", f.base);
  f.write("README.md", "# Feature\n");
  assert.deepEqual(changedFiles(f.cwd, "target").files, ["README.md"]);
});

test("shared configuration, templates, unknown paths and report schemas broaden validation", () => {
  for (const file of ["pnpm-lock.yaml", "Cargo.lock", "Cargo.toml", "package.json", "quality.yml", "turbo.json",
    "scripts/new.mjs", "templates/github-actions.yml", ".github/workflows/ci.yml", "new-workspace/file.ts",
    "apps/site/public/quality-report.schema.json"]) {
    assert.equal(classify([file]).fullReasons.length, 1, file);
  }
});

test("root documentation does not select workspace tasks; playground keeps its own checks", () => {
  assert.deepEqual(classify(["README.md", "docs/benchmark.md", ".changeset/pilot.md"]), { packages: [], playground: false, fullReasons: [] });
  assert.equal(classify(["playground/fixture/tool.py"]).playground, true);
});

test("unknown or missing Git base falls back to full validation and browser checks", (t) => {
  const f = fixture(t);
  const plan = createPlan({ cwd: f.cwd, base: "missing-ref" });
  assert.equal(plan.mode, "full");
  assert.equal(plan.docsBrowser, true);
  assert.deepEqual(plan.steps.map((step) => step.args), [["run", "validate"]]);
});

test("full mode preserves the canonical gate even on a clean checkout", (t) => {
  const f = fixture(t);
  const plan = createPlan({ cwd: f.cwd, full: true });
  assert.equal(plan.mode, "full");
  assert.equal(plan.docsBrowser, true);
  assert.equal(plan.steps[0].id, "validate");
});

test("Git discovery uses the requested checkout despite inherited hook variables", (t) => {
  const f = fixture(t);
  f.write("README.md", "# Changed\n");
  const original = process.env.GIT_DIR;
  process.env.GIT_DIR = "/missing/git-directory";
  try { assert.deepEqual(changedFiles(f.cwd, f.base).files, ["README.md"]); }
  finally {
    if (original === undefined) delete process.env.GIT_DIR;
    else process.env.GIT_DIR = original;
  }
});

test("no changes still runs the live audit and repository contracts", (t) => {
  const f = fixture(t);
  const plan = createPlan({ cwd: f.cwd, base: f.base });
  assert.equal(plan.mode, "affected");
  assert.equal(plan.docsBrowser, false);
  assert.deepEqual(plan.steps.map((step) => step.id), ["check", "security", "versions"]);
});

test("Turbo includes local Action edits and transitive site dependents", (t) => {
  const f = fixture(t, { dependent: true });
  f.write("packages/action/src/source.ts", "export const changed = true;\n");
  const plan = createPlan({ cwd: f.cwd, base: f.base });
  assert.equal(plan.mode, "affected");
  assert.deepEqual(plan.packages.sort(), ["@quality/action", "@quality/site"]);
  assert.equal(plan.docsBrowser, true);
  assert.ok(plan.steps.some((step) => step.id === "action-dist"));
  assert.ok(!plan.steps.some((step) => step.id === "playground"));
});

test("site edits select browser verification without CLI or Action checks", (t) => {
  const f = fixture(t);
  f.write("apps/site/src/source.ts", "export const changed = true;\n");
  const plan = createPlan({ cwd: f.cwd, base: f.base });
  assert.equal(plan.mode, "affected");
  assert.deepEqual(plan.packages, ["@quality/site"]);
  assert.equal(plan.docsBrowser, true);
  assert.deepEqual(plan.steps.map((step) => step.id), ["check", "packages", "security", "versions"]);
});

test("CLI deletions select tests, playground and the full analyzer check", (t) => {
  const f = fixture(t);
  rmSync(join(f.cwd, "crates/quality-cli/src/source.ts"));
  const plan = createPlan({ cwd: f.cwd, base: f.base });
  assert.equal(plan.mode, "affected");
  assert.deepEqual(plan.packages, ["@quality/cli"]);
  assert.deepEqual(plan.steps[0].args, ["run", "check"]);
  assert.ok(plan.steps.some((step) => step.id === "playground"));
});

test("malformed Turbo output fails instead of silently selecting no work", () => {
  for (const value of ["invalid", "null", "{}", '{"packages":[42]}']) {
    assert.throws(() => turboPackages(value));
  }
});

test("untracked workspace files participate even with an inherited Turbo head", (t) => {
  const f = fixture(t);
  f.write("packages/action/src/new.ts");
  const original = process.env.TURBO_SCM_HEAD;
  process.env.TURBO_SCM_HEAD = f.base;
  try {
    const plan = createPlan({ cwd: f.cwd, base: f.base });
    assert.equal(plan.mode, "affected");
    assert.deepEqual(plan.packages, ["@quality/action"]);
  } finally {
    if (original === undefined) delete process.env.TURBO_SCM_HEAD;
    else process.env.TURBO_SCM_HEAD = original;
  }
});

test("an unavailable Turbo installation broadens validation", (t) => {
  const f = fixture(t);
  f.write("apps/site/src/source.ts", "export const changed = true;\n");
  rmSync(join(f.cwd, "node_modules"));
  const plan = createPlan({ cwd: f.cwd, base: f.base });
  assert.equal(plan.mode, "full");
  assert.equal(plan.docsBrowser, true);
  assert.ok(plan.reasons.includes("Turbo planning unavailable; running full validation"));
});

test("a failed task stops execution and preserves its exit code", () => {
  const calls = [];
  const plan = { steps: [{ id: "first", args: ["first"], reason: "test" }, { id: "second", args: ["second"], reason: "test" }] };
  const result = runPlan(plan, { execute: (command, args) => { calls.push([command, args]); return { status: 7 }; } });
  assert.equal(result.status, 7);
  assert.equal(calls.length, 1);
  assert.equal(result.steps.length, 1);
});

test("release and pre-push gates stay full; browser job keeps a stable required check", () => {
  const manifest = JSON.parse(readFileSync(join(repository, "package.json"), "utf8"));
  assert.equal(manifest.scripts["pre-push"], "pnpm run validate");
  assert.ok(!manifest.scripts.validate.includes("affected"));
  const release = readFileSync(join(repository, ".github/workflows/release.yml"), "utf8");
  assert.match(release, /run: pnpm validate/);
  const ci = readFileSync(join(repository, ".github/workflows/ci.yml"), "utf8");
  assert.match(ci, /startsWith\(github.head_ref, 'release\/'\)/);
  assert.match(ci, /github.event_name == 'workflow_dispatch'/);
  assert.match(ci, /run: pnpm run ci:plan --github-output/);
  assert.equal(ci.match(/if: steps.plan.outputs.docs-browser == 'true'/g)?.length, 3);
});
