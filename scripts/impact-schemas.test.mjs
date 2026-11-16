import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import Ajv2020 from "ajv/dist/2020.js";

const repository = fileURLToPath(new URL("../", import.meta.url));
const names = {
  config: "quality-impact.schema.json",
  plan: "quality-impact-plan.schema.json",
  results: "quality-impact-results.schema.json",
  verification: "quality-impact-verification.schema.json",
};
const ajv = new Ajv2020({ allErrors: true, strict: true });
for (const filename of Object.values(names)) {
  ajv.addSchema(JSON.parse(readFileSync(join(repository, "apps/site/public", filename), "utf8")));
}
const validators = Object.fromEntries(Object.entries(names).map(([kind, filename]) => {
  const validate = ajv.getSchema(`https://quality.santi020k.com/${filename}`);
  assert.ok(validate, `compile ${filename}`);
  return [kind, validate];
}));

function valid(kind, value) {
  assert.equal(validators[kind](value), true, JSON.stringify(validators[kind].errors));
}

function invalid(kind, value, description) {
  assert.equal(validators[kind](value), false, description);
}

const commit = "a".repeat(40);
const planId = `git-blob:${"b".repeat(40)}`;
const config = {
  schema_version: 1,
  global_inputs: ["package.json"],
  ignored_inputs: ["docs/"],
  nodes: {
    audit: { always: true, command: "pnpm", args: ["audit"] },
    build: { inputs: ["source.txt"], command: "pnpm", args: ["build"] },
    browser: { depends_on: ["build"], command: "pnpm", args: ["test:browser"] },
  },
};
const plan = {
  schema_version: 1,
  quality_version: "2.0.0",
  plan_id: planId,
  config_path: "quality-impact.json",
  config,
  source: {
    requested_base: "main",
    requested_head: "HEAD",
    comparison: "merge-base",
    base: commit,
    head: commit,
    merge_base: commit,
  },
  full_requested: false,
  mode: "affected",
  changed_paths: ["source.txt"],
  reasons: [],
  selected: [
    { id: "audit", command: "pnpm", args: ["audit"], depends_on: [], reasons: [{ kind: "always" }] },
    { id: "build", command: "pnpm", args: ["build"], depends_on: [], reasons: [{ kind: "input", path: "source.txt" }] },
    { id: "browser", command: "pnpm", args: ["test:browser"], depends_on: ["build"], reasons: [{ kind: "dependent", node: "build" }] },
  ],
  skipped: [],
  batches: [["audit", "build"], ["browser"]],
};
const results = {
  schema_version: 1,
  plan_id: planId,
  head: commit,
  results: plan.selected.map(({ id }) => ({ id, status: "passed", exit_code: 0 })),
};
const verification = {
  schema_version: 1,
  plan_id: planId,
  head: commit,
  passed: true,
  checked_nodes: 3,
  failures: [],
};

test("impact schemas validate repository policy and complete contract fixtures", () => {
  valid("config", JSON.parse(readFileSync(join(repository, "quality-impact.json"), "utf8")));
  valid("config", config);
  valid("plan", plan);
  valid("results", results);
  valid("verification", verification);
  for (const kind of Object.keys(names)) {
    const fixture = { config, plan, results, verification }[kind];
    invalid(kind, { ...fixture, schema_version: 2 }, `${kind}: schema version`);
    invalid(kind, { ...fixture, unexpected: true }, `${kind}: unknown property`);
  }
});

test("impact configuration enforces identifiers, paths, commands and argument constraints", () => {
  invalid("config", { schema_version: 1, nodes: {} }, "empty graph");
  for (const id of ["", "Uppercase", "with space", "line\n"]) {
    invalid("config", { schema_version: 1, nodes: { [id]: { command: "check" } } }, `node: ${JSON.stringify(id)}`);
  }
  for (const path of ["", "../escape", "/absolute", "src//", "src/*", "src/?", "src/[x]", "C:/repo", "src\\file", "src/../file", "src/./file", "src/\nfile"]) {
    invalid("config", { ...config, global_inputs: [path] }, `input: ${JSON.stringify(path)}`);
  }
  for (const command of ["", " ", "\u00a0", "check\n", "check\u0000"]) {
    invalid("config", { schema_version: 1, nodes: { check: { command } } }, `command: ${JSON.stringify(command)}`);
  }
  invalid("config", { schema_version: 1, nodes: { check: { command: "check", args: ["bad\u0000argument"] } } }, "NUL argument");
  invalid("config", { schema_version: 1, nodes: { check: { command: "check", working_directory: "../outside" } } }, "escaping working directory");
  invalid("config", { schema_version: 1, nodes: { check: { command: "check", unknown: true } } }, "unknown node property");
  valid("config", {
    schema_version: 1,
    global_inputs: ["src/", "src/", "á file.ts"],
    nodes: { check: { command: "check", args: ["", "line\nbreak"], working_directory: "src//" } },
  });
});

test("impact plans enforce reason variants, batches, identities and conservative modes", () => {
  for (const mutate of [
    (value) => { value.plan_id = "git-blob:short"; },
    (value) => { value.source.head = `${commit}\n`; },
    (value) => { value.source.comparison = "unknown"; },
    (value) => { value.selected[0].reasons = [{ kind: "unknown" }]; },
    (value) => { value.selected[0].reasons = [{ kind: "input" }]; },
    (value) => { value.selected[0].reasons = [{ kind: "always", path: "extra" }]; },
    (value) => { value.selected[0].reasons = []; },
    (value) => { value.batches = [[]]; },
    (value) => { value.batches = [["audit", "audit"]]; },
    (value) => { value.changed_paths = ["../outside"]; },
    (value) => { value.changed_paths = ["source/"]; },
    (value) => { value.changed_paths.push("source.txt"); },
    (value) => { value.mode = "full"; },
    (value) => { value.full_requested = true; },
    (value) => { value.reasons = [{ kind: "explicit_full" }]; },
    (value) => { delete value.config_path; },
  ]) {
    const altered = structuredClone(plan);
    mutate(altered);
    invalid("plan", altered, JSON.stringify(altered));
  }
  const full = structuredClone(plan);
  full.mode = "full";
  full.full_requested = true;
  full.reasons = [{ kind: "explicit_full" }, { kind: "global_input", path: "package.json" }];
  full.source.base = null;
  full.source.merge_base = null;
  valid("plan", full);
  const unavailable = structuredClone(full);
  unavailable.source.head = null;
  unavailable.reasons.push({ kind: "head_unavailable" });
  valid("plan", unavailable);
  valid("plan", { ...plan, changed_paths: ["src/[literal].ts", "src/.\n", "back\\slash", "line\nbreak"] });
  valid("plan", { ...plan, plan_id: `git-blob:${"b".repeat(64)}`, source: { ...plan.source, head: "a".repeat(64) } });
});

test("impact result success requires an exact zero exit and bounded integer values", () => {
  for (const record of [
    { id: "audit", status: "passed" },
    { id: "audit", status: "passed", exit_code: null },
    { id: "audit", status: "passed", exit_code: 1 },
    { id: "audit", status: "failed", exit_code: 1.5 },
    { id: "audit", status: "failed", exit_code: 2147483648 },
    { id: "audit", status: "failed", exit_code: -2147483649 },
    { id: "audit", status: "pending", exit_code: 0 },
    { id: "audit", status: "passed", exit_code: 0, unexpected: true },
  ]) {
    invalid("results", { ...results, results: [record] }, JSON.stringify(record));
  }
  for (const status of ["failed", "cancelled", "skipped"]) {
    valid("results", { ...results, results: [{ id: "audit", status }] });
    valid("results", { ...results, results: [{ id: "audit", status, exit_code: null }] });
  }
});

test("impact verification reports enforce counts and pass/failure consistency", () => {
  invalid("verification", { ...verification, checked_nodes: -1 }, "negative count");
  invalid("verification", { ...verification, checked_nodes: 0.5 }, "fractional count");
  invalid("verification", { ...verification, passed: false }, "failure without reasons");
  invalid("verification", { ...verification, failures: [{ id: "audit", reason: "missing result" }] }, "success with failures");
  invalid("verification", { ...verification, passed: false, failures: [{ id: "audit", reason: "unknown" }] }, "unknown failure reason");
  valid("verification", { ...verification, passed: false, failures: [{ id: "audit", reason: "missing result" }] });
});

test("real CLI affected/full plans and successful/failed verification conform to public schemas", () => {
  const root = mkdtempSync(join(tmpdir(), "quality-impact-schema-"));
  const environment = { ...process.env };
  for (const key of ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG_COUNT", "GIT_CONFIG_PARAMETERS"]) {
    delete environment[key];
  }
  function git(...args) {
    const output = spawnSync("git", ["-C", root, ...args], { encoding: "utf8", env: environment });
    assert.equal(output.status, 0, output.error?.message ?? output.stderr);
    return output.stdout.trim();
  }
  function quality(expectedExit, ...args) {
    const output = spawnSync("cargo", ["run", "--locked", "--quiet", "--package", "quality-cli", "--", "--root", root, ...args, "--format", "json"], {
      cwd: repository,
      encoding: "utf8",
      env: environment,
      maxBuffer: 16 * 1024 * 1024,
    });
    assert.equal(output.status, expectedExit, output.error?.message ?? output.stderr);
    return JSON.parse(output.stdout);
  }
  try {
    git("init", "--quiet");
    git("config", "user.name", "Impact schema tests");
    git("config", "user.email", "impact@example.invalid");
    git("config", "commit.gpgSign", "false");
    git("config", "core.hooksPath", join(root, "absent-hooks"));
    writeFileSync(join(root, "quality-impact.json"), JSON.stringify(config));
    writeFileSync(join(root, "source.txt"), "initial\n");
    git("add", ".");
    git("commit", "--quiet", "-m", "Initial fixture");
    const base = git("rev-parse", "HEAD");
    writeFileSync(join(root, "source.txt"), "changed\n");
    git("add", ".");
    git("commit", "--quiet", "-m", "Changed fixture");
    const actualPlan = quality(0, "impact", "plan", "--base", base);
    valid("plan", actualPlan);
    valid("plan", quality(0, "impact", "plan", "--full"));
    valid("plan", quality(0, "impact", "plan", "--full", "--head", "missing-ref"));
    const actualResults = {
      schema_version: 1,
      plan_id: actualPlan.plan_id,
      head: actualPlan.source.head,
      results: actualPlan.selected.map(({ id }) => ({ id, status: "passed", exit_code: 0 })),
    };
    valid("results", actualResults);
    writeFileSync(join(root, "plan.json"), JSON.stringify(actualPlan));
    writeFileSync(join(root, "results.json"), JSON.stringify(actualResults));
    valid("verification", quality(0, "impact", "verify", "--plan", "plan.json", "--results", "results.json"));
    actualResults.results = [];
    writeFileSync(join(root, "results.json"), JSON.stringify(actualResults));
    valid("verification", quality(1, "impact", "verify", "--plan", "plan.json", "--results", "results.json"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
