import { spawnSync } from "node:child_process";
import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { cpus, platform, arch, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { createPlan, gitEnvironment } from "./affected-ci.mjs";

const scenarios = [
  { name: "readme", path: "README.md", text: "\n<!-- Quality benchmark. -->\n" },
  { name: "action-test", path: "packages/action/src/lib.test.ts", text: "\n// Quality benchmark.\n" },
  { name: "site-content", path: "apps/site/src/content/docs/commands.md", text: "\n<!-- Quality benchmark. -->\n" },
  { name: "rust-source", path: "crates/quality-cli/src/changes.rs", text: "\n// Quality benchmark.\n" },
  { name: "shared-config", path: "quality.yml", text: "\n# Quality benchmark.\n" },
];

export function deterministicSteps(plan, validate) {
  if (plan.mode !== "full") return plan.steps.filter((step) => step.id !== "security");
  return validate.split(" && ").map((command) => {
    const match = /^pnpm run ([a-z:-]+)$/.exec(command);
    if (!match) throw new Error(`Unsupported validation command in benchmark: ${command}`);
    return { id: match[1], args: ["run", match[1]] };
  }).filter((step) => step.id !== "check:security");
}

function command(executable, args, cwd, env, log) {
  const start = performance.now();
  const result = spawnSync(executable, args, { cwd, env, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  if (log) writeFileSync(log, output);
  if (result.error) throw result.error;
  return { seconds: (performance.now() - start) / 1000, status: result.status ?? 1, output };
}

function main() {
  const { values } = parseArgs({ options: {
    output: { type: "string" }, repetitions: { type: "string", default: "2" }, scenario: { type: "string" },
  } });
  const repetitions = Number(values.repetitions);
  if (!Number.isInteger(repetitions) || repetitions < 1 || repetitions > 10) throw new Error("Use 1–10 warm repetitions");
  const selected = scenarios.filter((scenario) => !values.scenario || scenario.name === values.scenario);
  if (selected.length === 0) throw new Error("Unknown benchmark scenario");
  const source = resolve(import.meta.dirname, "..");
  const gitPath = command("git", ["rev-parse", "--git-path", "quality/benchmarks"], source, gitEnvironment());
  if (gitPath.status !== 0) throw new Error("Benchmark needs a Git checkout");
  const outputDirectory = values.output ? resolve(values.output) :
    resolve(source, gitPath.output.trim(), new Date().toISOString().replaceAll(":", "-"));
  mkdirSync(outputDirectory, { recursive: true });
  const temporary = mkdtempSync(join(tmpdir(), "quality-ci-benchmark-"));
  const cwd = join(temporary, "repository");
  mkdirSync(cwd);
  const env = { ...gitEnvironment(), TURBO_TELEMETRY_DISABLED: "1", TURBO_CACHE: "local:rw", CARGO_TARGET_DIR: join(source, "target") };
  for (const key of ["TURBO_SCM_BASE", "TURBO_SCM_HEAD", "TURBO_TOKEN", "TURBO_TEAM", "TURBO_API"]) delete env[key];
  const must = (executable, args, directory = cwd, log) => {
    const result = command(executable, args, directory, env, log);
    if (result.status !== 0) throw new Error(`${executable} ${args.join(" ")} failed; ${log ?? result.output.slice(-2000)}`);
    return result;
  };
  const report = {
    version: 1, createdAt: new Date().toISOString(), node: process.version, platform: platform(), architecture: arch(),
    cpu: cpus()[0]?.model, repetitions, measurements: [],
    scope: "Deterministic validation only. Live audit and browser suite measured separately. Cold means an empty local Turbo task cache; dependencies and compiler caches are prewarmed. No remote cache.",
  };
  const save = () => writeFileSync(join(outputDirectory, "results.json"), `${JSON.stringify(report, null, 2)}\n`);
  try {
    // Copy versioned and new source files, including this uncommitted pilot.
    // Ignored secrets, build products and dependencies are never copied.
    const files = must("git", ["ls-files", "--cached", "--others", "--exclude-standard", "-z"], source).output;
    for (const file of new Set(files.split("\0").filter(Boolean))) {
      if (!existsSync(join(source, file))) continue;
      mkdirSync(dirname(join(cwd, file)), { recursive: true });
      copyFileSync(join(source, file), join(cwd, file));
    }
    must("git", ["init", "-q"]);
    must("git", ["config", "user.name", "Quality benchmark"]);
    must("git", ["config", "user.email", "benchmark@example.invalid"]);
    must("git", ["add", "."]);
    must("git", ["commit", "-qm", "Benchmark snapshot"]);
    const base = must("git", ["rev-parse", "HEAD"]).output.trim();
    report.sourceRevision = must("git", ["rev-parse", "HEAD"], source).output.trim();
    report.snapshotRevision = base;
    console.log(`Benchmark evidence: ${outputDirectory}`);
    must("pnpm", ["install", "--frozen-lockfile", "--offline"], cwd, join(outputDirectory, "install.log"));
    const validate = JSON.parse(readFileSync(join(cwd, "package.json"), "utf8")).scripts.validate;
    const full = createPlan({ cwd, full: true });
    const measure = (plan, name, cache) => {
      const steps = [];
      const start = performance.now();
      for (const step of deterministicSteps(plan, validate)) {
        const log = join(outputDirectory, `${name}-${step.id.replaceAll(":", "-")}.log`);
        const result = command("pnpm", step.args, cwd, { ...env, TURBO_CACHE_DIR: cache }, log);
        steps.push({ id: step.id, seconds: result.seconds, status: result.status,
          cache: result.output.replace(/\x1b\[[0-9;]*m/g, "").split("\n").filter((line) => /^\s*Cached:/.test(line)) });
        if (result.status !== 0) throw new Error(`Benchmark step failed: ${log}`);
      }
      return { seconds: (performance.now() - start) / 1000, steps };
    };
    for (const scenario of selected) {
      const original = readFileSync(join(cwd, scenario.path));
      appendFileSync(join(cwd, scenario.path), scenario.text);
      console.log(`Preparing ${scenario.name} (compiler warmup).`);
      measure(full, `${scenario.name}-warmup`, join(temporary, "warmup-cache"));
      const planStart = performance.now();
      const affected = createPlan({ cwd, base });
      const planningSeconds = (performance.now() - planStart) / 1000;
      writeFileSync(join(outputDirectory, `${scenario.name}-plan.json`), JSON.stringify(affected, null, 2));
      for (let repetition = 0; repetition <= repetitions; repetition++) {
        // Alternate order to reduce systematic first-run bias.
        const modes = repetition % 2 === 0 ? ["full", "affected"] : ["affected", "full"];
        for (const mode of modes) {
          const name = `${scenario.name}-${mode}-${repetition}`;
          const result = measure(mode === "full" ? full : affected, name, join(temporary, `${scenario.name}-${mode}-cache`));
          const measurement = { scenario: scenario.name, mode, cache: repetition === 0 ? "cold" : "warm", repetition,
            selectedMode: mode === "full" ? "full" : affected.mode, docsBrowser: mode === "full" || affected.docsBrowser,
            planningSeconds: mode === "full" ? 0 : planningSeconds, ...result };
          report.measurements.push(measurement);
          save();
          console.log(`${name}: ${result.seconds.toFixed(2)}s (${measurement.selectedMode})`);
        }
      }
      writeFileSync(join(cwd, scenario.path), original);
      const dirty = must("git", ["status", "--porcelain"]).output.trim();
      if (dirty) throw new Error(`Benchmark produced unexpected source changes: ${dirty}`);
    }
    const audit = command("pnpm", ["run", "check:security"], cwd, env, join(outputDirectory, "security.log"));
    report.audit = { seconds: audit.seconds, status: audit.status };
    const browser = command("pnpm", ["test:site"], cwd, { ...env, CI: "true" }, join(outputDirectory, "browser.log"));
    report.browser = { seconds: browser.seconds, status: browser.status };
    save();
    console.log(`Audit exit: ${audit.status}; browser exit: ${browser.status}. Results: ${join(outputDirectory, "results.json")}`);
    // Timing evidence is retained, but a failing gate is never reported as success.
    process.exitCode = audit.status || browser.status;
  } finally {
    save();
    rmSync(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(); }
  catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}
