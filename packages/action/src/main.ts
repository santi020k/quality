import * as core from "@actions/core";
import * as exec from "@actions/exec";
import * as cache from "@actions/tool-cache";
import { chmod, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import actionPackage from "../package.json" with { type: "json" };
import {
  checkArguments,
  actionFailureMessage,
  cacheToolName,
  checksumFromFile,
  releaseBaseUrl,
  releaseRepository,
  releaseTarget,
  releaseVersion,
  resolveProjectPath,
  sha256,
  summarizeSarifReport,
  verifyBinaryVersion,
} from "./lib.js";

async function verifyInstalledVersion(binary: string, requested: string): Promise<void> {
  const result = await exec.getExecOutput(binary, ["--version"], {
    silent: true,
    ignoreReturnCode: true,
  });
  if (result.exitCode !== 0) {
    throw new Error(`installed quality binary could not report its version (exit ${result.exitCode})`);
  }
  verifyBinaryVersion(result.stdout, requested);
}

async function installQuality(version: string): Promise<string> {
  const target = releaseTarget(process.platform, process.arch);
  const windows = process.platform === "win32";
  const binaryName = windows ? "quality.exe" : "quality";
  // tool-cache normalizes away SemVer build metadata, which can alias distinct
  // immutable release tags. Install these exact versions without that cache.
  const cacheVersion = version === "latest" || version.includes("+") ? "" : version;

  const repository = releaseRepository(
    process.env.GITHUB_ACTION_REPOSITORY,
    process.env.QUALITY_ACTION_REPOSITORY,
  );
  const resolvedBaseUrl = releaseBaseUrl(repository, process.env.QUALITY_RELEASE_BASE_URL);
  const toolName = cacheToolName(resolvedBaseUrl);

  const cached = cacheVersion ? cache.find(toolName, cacheVersion, target) : "";
  if (cached) {
    const binary = path.join(cached, binaryName);
    await verifyInstalledVersion(binary, version);
    return binary;
  }

  const release = version === "latest" ? "releases/latest/download" : `releases/download/${version}`;
  const asset = `quality-${target}.${windows ? "zip" : "tar.gz"}`;
  const base = `${resolvedBaseUrl}/${release}`;

  core.info(`Installing quality ${version} for ${target}`);
  const archive = await cache.downloadTool(`${base}/${asset}`);
  const checksumFile = await cache.downloadTool(`${base}/${asset}.sha256`);
  const expected = checksumFromFile(await readFile(checksumFile, "utf8"), asset);
  const actual = await sha256(archive);
  if (actual !== expected) throw new Error(`checksum mismatch for ${asset}`);

  const extracted = windows ? await cache.extractZip(archive) : await cache.extractTar(archive);
  const binary = path.join(extracted, binaryName);
  if (!windows) await chmod(binary, 0o755);
  await verifyInstalledVersion(binary, version);
  if (!cacheVersion) return binary;
  const cachedDirectory = await cache.cacheFile(
    binary,
    binaryName,
    toolName,
    cacheVersion,
    target,
  );
  return path.join(cachedDirectory, binaryName);
}

async function run(): Promise<void> {
  const started = Date.now();
  let temporaryReportDirectory: string | undefined;
  try {
    const version = releaseVersion(core.getInput("version"), actionPackage.version);
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
    if (!sarifInput) {
      temporaryReportDirectory = await mkdtemp(
        path.join(process.env.RUNNER_TEMP || tmpdir(), "quality-action-"),
      );
    }
    const sarif = temporaryReportDirectory
      ? path.join(temporaryReportDirectory, "quality.sarif")
      : path.resolve(workingDirectory, sarifInput);
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
    const binary = await installQuality(version);
    const result = await exec.getExecOutput(binary, args, {
      cwd: workingDirectory,
      ignoreReturnCode: true,
    });

    if (result.exitCode !== 0 && result.exitCode !== 1) {
      core.setFailed(actionFailureMessage(result.exitCode, result.stderr));
      return;
    }

    const report: unknown = JSON.parse(await readFile(sarif, "utf8"));
    const summary = summarizeSarifReport(report);
    if (sarifInput) core.setOutput("sarif", sarif);
    core.setOutput("findings", summary.findings);
    core.setOutput("tools", summary.tools);

    if (result.exitCode !== 0) {
      core.setFailed(actionFailureMessage(result.exitCode, result.stderr));
    }
  } catch (error) {
    core.setFailed(error instanceof Error ? error.message : String(error));
  } finally {
    if (temporaryReportDirectory) {
      try {
        await rm(temporaryReportDirectory, { recursive: true, force: true });
      } catch {
        core.warning("Could not remove the temporary quality report from the runner's temporary directory.");
      }
    }
    core.setOutput("duration-ms", Date.now() - started);
  }
}

void run();
