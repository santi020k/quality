import { spawnSync } from "node:child_process";
import { appendFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const workspaces = new Map([
  ["apps/site/", "@quality/site"],
  ["crates/quality-cli/", "@quality/cli"],
  ["packages/action/", "@quality/action"],
]);

export function gitEnvironment(environment = process.env) {
  const env = { ...environment };
  for (const key of ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"]) delete env[key];
  return env;
}

function capture(command, args, cwd, env = process.env) {
  const result = spawnSync(command, args, { cwd, env, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} failed: ${result.error?.message ?? (result.stderr || result.stdout).trim()}`);
  }
  return result.stdout;
}

export function changedFiles(cwd, base) {
  const git = (args) => capture("git", args, cwd, gitEnvironment());
  const revision = git(["rev-parse", "--verify", "--end-of-options", `${base}^{commit}`]).trim();
  const ancestor = git(["merge-base", revision, "HEAD"]).trim();
  // Disable rename detection so both the removed and added paths affect selection.
  // Diff against the worktree includes staged and unstaged changes and deletions.
  const tracked = git(["diff", "--name-only", "--no-renames", "-z", ancestor, "--"]);
  const untracked = git(["ls-files", "--others", "--exclude-standard", "-z"]);
  return { base: revision, files: [...new Set(`${tracked}${untracked}`.split("\0").filter(Boolean))].sort() };
}

export function classify(files) {
  const packages = new Set();
  const fullReasons = [];
  let playground = false;
  for (const file of files) {
    // These schemas are also compiled into the Rust integration tests.
    if (/^apps\/site\/public\/quality.*\.schema\.json$/.test(file)) {
      fullReasons.push(`Shared report contract: ${file}`);
    } else if (file.startsWith("playground/")) {
      playground = true;
    } else {
      const workspace = [...workspaces].find(([prefix]) => file.startsWith(prefix));
      if (workspace) packages.add(workspace[1]);
      else if (!/^(README\.md|CONTRIBUTING\.md|CHANGELOG\.md|LICENSE|docs\/.*\.(md|svg|png|jpg|webp)|\.changeset\/[^/]+\.md)$/.test(file)) {
        fullReasons.push(`Shared or unclassified input: ${file}`);
      }
    }
  }
  return { packages: [...packages].sort(), playground, fullReasons };
}

export function turboPackages(source) {
  const report = JSON.parse(source);
  if (typeof report !== "object" || report === null || !Array.isArray(report.packages) ||
      !report.packages.every((name) => typeof name === "string")) {
    throw new Error("Turbo returned an invalid package plan");
  }
  return report.packages.filter((name) => name !== "//");
}

export function createPlan({ cwd = process.cwd(), base = "origin/main", full = false } = {}) {
  let changes = { base: null, files: [] };
  let reasons = full ? ["Full validation explicitly requested"] : [];
  if (!full) {
    try { changes = changedFiles(cwd, base); }
    catch { reasons = ["Git comparison unavailable; running full validation"]; }
  }
  const scope = classify(changes.files);
  reasons.push(...scope.fullReasons);
  let packages = [];
  if (reasons.length === 0 && scope.packages.length > 0) {
    const env = { ...gitEnvironment(), TURBO_SCM_BASE: changes.base };
    // An inherited head would hide local edits. This entry point always checks
    // the current checkout, including its working tree.
    delete env.TURBO_SCM_HEAD;
    // Invoke Turbo's installed Node entry point so a read-only plan cannot
    // trigger pnpm's automatic dependency installation.
    try {
      packages = turboPackages(capture(process.execPath, [resolve(cwd, "node_modules/turbo/bin/turbo"), "run", "test", "build", "--affected", "--dry=json", "--cache=local:"], cwd, env));
    } catch {
      reasons.push("Turbo planning unavailable; running full validation");
    }
    if (scope.packages.some((name) => !packages.includes(name))) {
      reasons.push("Turbo did not include every directly changed workspace; running full validation");
    }
    if (packages.some((name) => ![...workspaces.values()].includes(name))) {
      reasons.push("Workspace layout changed; running full validation");
    }
  }
  const mode = reasons.length > 0 ? "full" : "affected";
  const cli = packages.includes("@quality/cli");
  const action = packages.includes("@quality/action");
  const docsBrowser = mode === "full" || packages.includes("@quality/site");
  const steps = mode === "full" ? [
    { id: "validate", args: ["run", "validate"], reason: reasons.join("; ") },
  ] : [
    { id: "check", args: ["run", "check", ...(cli ? [] : ["--changed", changes.base])], reason: cli ? "CLI changed; exercise all analyzers" : "Quality selects applicable changed inputs" },
    ...(packages.length > 0 ? [{ id: "packages", args: ["exec", "turbo", "run", "test", "build", ...packages.map((name) => `--filter=${name}`)], reason: `Turbo affected packages: ${packages.join(", ")}` }] : []),
    ...(cli || scope.playground ? [{ id: "playground", args: ["run", "playground:verify"], reason: "CLI or playground affected" }] : []),
    // Advisory data can change independently of Git; never cache or skip this.
    { id: "security", args: ["run", "check:security"], reason: "Live dependency audit" },
    { id: "versions", args: ["run", "check:versions"], reason: "Release references and repository script contracts" },
    ...(action ? [{ id: "action-dist", args: ["run", "check:action-dist"], reason: "Action source or generated bundle affected" }] : []),
  ];
  return {
    version: 1, mode, requestedBase: base, base: changes.base, files: changes.files,
    packages: mode === "full" ? [...workspaces.values()] : packages,
    docsBrowser, reasons, steps,
    skipped: mode === "full" ? [] : [
      ...(!cli && !scope.playground ? ["playground: CLI and fixture unchanged"] : []),
      ...(!action ? ["action-dist: Action unchanged"] : []),
      ...(!docsBrowser ? ["docs-browser: documentation application unchanged"] : []),
      ...(packages.length === 0 ? ["packages: no workspace inputs changed"] : []),
    ],
  };
}

export function runPlan(plan, { cwd = process.cwd(), execute = spawnSync } = {}) {
  const results = [];
  for (const step of plan.steps) {
    console.log(`\n[${step.id}] ${step.reason}\npnpm ${step.args.join(" ")}`);
    const start = performance.now();
    const result = execute("pnpm", step.args, { cwd, stdio: "inherit", env: process.env });
    const status = result.status ?? 1;
    results.push({ id: step.id, seconds: (performance.now() - start) / 1000, status });
    if (result.error) console.error(result.error.message);
    if (status !== 0) return { status, steps: results };
  }
  return { status: 0, steps: results };
}

function main() {
  const { values } = parseArgs({ options: {
    base: { type: "string", default: process.env.QUALITY_CI_BASE || "origin/main" },
    full: { type: "boolean", default: process.env.QUALITY_CI_FULL === "true" },
    plan: { type: "boolean", default: false },
    "github-output": { type: "boolean", default: false },
    report: { type: "string" },
  } });
  const start = performance.now();
  const plan = createPlan(values);
  if (values["github-output"]) {
    if (!process.env.GITHUB_OUTPUT) throw new Error("GITHUB_OUTPUT is required");
    appendFileSync(process.env.GITHUB_OUTPUT, `docs-browser=${plan.docsBrowser}\nmode=${plan.mode}\n`);
  }
  if (values.plan) {
    console.log(JSON.stringify(plan, null, 2));
    return;
  }
  console.log(`Quality repository pilot: ${plan.mode} (${plan.files.length} changed paths)`);
  for (const reason of plan.skipped) console.log(`Skip ${reason}`);
  const result = runPlan(plan);
  const report = { ...plan, results: result.steps, status: result.status, seconds: (performance.now() - start) / 1000 };
  if (values.report) writeFileSync(values.report, `${JSON.stringify(report, null, 2)}\n`);
  console.log(`\nPilot finished in ${report.seconds.toFixed(2)}s (exit ${result.status}).`);
  process.exitCode = result.status;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(); }
  catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}
