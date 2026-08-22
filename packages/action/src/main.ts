import * as core from "@actions/core";
import * as exec from "@actions/exec";
import * as cache from "@actions/tool-cache";
import { chmod, readFile } from "node:fs/promises";
import path from "node:path";
import {
  checkArguments,
  actionFailureMessage,
  checksumFromFile,
  releaseRepository,
  releaseTarget,
  resolveProjectPath,
  sha256,
} from "./lib.js";

async function installQuality(version: string): Promise<string> {
  const target = releaseTarget(process.platform, process.arch);
  const windows = process.platform === "win32";
  const binaryName = windows ? "quality.exe" : "quality";
  const cacheVersion = version === "latest" ? "" : version;
  const cached = cacheVersion ? cache.find("quality", cacheVersion, target) : "";
  if (cached) return path.join(cached, binaryName);

  const repository = releaseRepository(
    process.env.GITHUB_ACTION_REPOSITORY,
    process.env.QUALITY_ACTION_REPOSITORY,
  );
  const release = version === "latest" ? "releases/latest/download" : `releases/download/${version}`;
  const asset = `quality-${target}.${windows ? "zip" : "tar.gz"}`;
  const base = `https://github.com/${repository}/${release}`;

  core.info(`Installing quality ${version} for ${target}`);
  const archive = await cache.downloadTool(`${base}/${asset}`);
  const checksumFile = await cache.downloadTool(`${base}/${asset}.sha256`);
  const expected = checksumFromFile(await readFile(checksumFile, "utf8"), asset);
  const actual = await sha256(archive);
  if (actual !== expected) throw new Error(`checksum mismatch for ${asset}`);

  const extracted = windows ? await cache.extractZip(archive) : await cache.extractTar(archive);
  const binary = path.join(extracted, binaryName);
  if (!windows) await chmod(binary, 0o755);
  if (!cacheVersion) return binary;
  const cachedDirectory = await cache.cacheFile(
    binary,
    binaryName,
    "quality",
    cacheVersion,
    target,
  );
  return path.join(cachedDirectory, binaryName);
}

async function run(): Promise<void> {
  const started = Date.now();
  try {
    const version = core.getInput("version", { required: true });
    const workspace = process.env.GITHUB_WORKSPACE ?? process.cwd();
    const workingDirectory = resolveProjectPath(
      workspace,
      core.getInput("working-directory", { required: true }),
    );
    const explicitBase = core.getInput("base");
    const pullRequestBase = process.env.GITHUB_BASE_REF
      ? `origin/${process.env.GITHUB_BASE_REF}`
      : "";
    const base = explicitBase || pullRequestBase;
    const changedOnly = core.getBooleanInput("changed-only");
    if (changedOnly && !base) {
      core.info("No pull-request base is available; checking the complete project.");
    }

    const sarifInput = core.getInput("sarif");
    const sarif = sarifInput ? path.resolve(workingDirectory, sarifInput) : "";
    const binary = await installQuality(version);
    const args = checkArguments({
      base,
      changedOnly,
      reportLevel: core.getInput("report-level", { required: true }),
      failLevel: core.getInput("fail-level", { required: true }),
      sarif,
      requireChecks: core.getBooleanInput("require-checks"),
      jobs: core.getInput("jobs"),
      timeoutSeconds: core.getInput("timeout-seconds"),
      maxOutputBytes: core.getInput("max-output-bytes"),
    });
    const result = await exec.getExecOutput(binary, args, {
      cwd: workingDirectory,
      ignoreReturnCode: true,
    });

    let findings = 0;
    let tools = 0;
    if (sarif) {
      const report = JSON.parse(await readFile(sarif, "utf8")) as {
        runs?: Array<{ results?: unknown[] }>;
      };
      tools = report.runs?.length ?? 0;
      findings = report.runs?.reduce((count, entry) => count + (entry.results?.length ?? 0), 0) ?? 0;
      core.setOutput("sarif", sarif);
    }
    core.setOutput("findings", findings);
    core.setOutput("tools", tools);
    core.setOutput("duration-ms", Date.now() - started);

    if (result.exitCode !== 0) {
      core.setFailed(actionFailureMessage(result.exitCode, result.stderr));
    }
  } catch (error) {
    core.setFailed(error instanceof Error ? error.message : String(error));
  }
}

void run();
