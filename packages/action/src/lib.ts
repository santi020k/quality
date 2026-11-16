import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";

export type Platform =
  | "aarch64-apple-darwin"
  | "aarch64-unknown-linux-gnu"
  | "x86_64-apple-darwin"
  | "x86_64-pc-windows-msvc"
  | "x86_64-unknown-linux-gnu";

export function releaseRepository(
  actionRepository: string | undefined,
  fallbackRepository: string | undefined,
): string {
  const repository = actionRepository || fallbackRepository;
  if (!repository) throw new Error("release repository is unavailable");
  return repository;
}

export function releaseBaseUrl(repository: string, override: string | undefined): string {
  return (override || `https://github.com/${repository}`).replace(/\/+$/u, "");
}

export function releaseTarget(platform: NodeJS.Platform, arch: string): Platform {
  if (platform === "darwin" && arch === "arm64") return "aarch64-apple-darwin";
  if (platform === "darwin" && arch === "x64") return "x86_64-apple-darwin";
  if (platform === "linux" && arch === "x64") return "x86_64-unknown-linux-gnu";
  if (platform === "linux" && arch === "arm64") return "aarch64-unknown-linux-gnu";
  if (platform === "win32" && arch === "x64") return "x86_64-pc-windows-msvc";
  throw new Error(`quality does not publish a prebuilt binary for ${platform}/${arch}`);
}

export function checksumFromFile(contents: string, asset: string): string {
  const escaped = asset.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = contents.match(new RegExp(`^([a-fA-F0-9]{64})\\s+[*]?${escaped}$`, "m"));
  if (!match?.[1]) throw new Error(`checksum file does not contain an entry for ${asset}`);
  return match[1].toLowerCase();
}

export async function sha256(file: string): Promise<string> {
  const bytes = await readFile(file);
  return createHash("sha256").update(bytes).digest("hex");
}

/**
 * Tool-cache lookups and insertions are keyed only by tool name, version,
 * and arch, with no notion of which release repository or base URL the
 * binary came from. Two distinct repositories (or a fork publishing a
 * fixture server override) can therefore publish the same version string
 * and silently reuse each other's cached binary. Hashing the fully
 * resolved base URL into the tool name gives each source its own
 * namespace while staying a fixed-length, path-safe cache key.
 */
export function cacheToolName(resolvedBaseUrl: string): string {
  return `quality-${createHash("sha256").update(resolvedBaseUrl).digest("hex")}`;
}

export function checkArguments(inputs: {
  base: string;
  changedOnly: boolean;
  reportLevel: string;
  failLevel: string;
  sarif: string;
  requireChecks: boolean;
  jobs: string;
  timeoutSeconds: string;
  maxOutputBytes: string;
}): string[] {
  const args = [
    "check",
    "--format",
    "github",
    "--report-level",
    inputs.reportLevel,
    "--fail-level",
    inputs.failLevel,
  ];
  if (inputs.requireChecks) args.push("--require-checks");
  if (inputs.jobs) args.push("--jobs", inputs.jobs);
  if (inputs.timeoutSeconds) args.push("--timeout-seconds", inputs.timeoutSeconds);
  if (inputs.maxOutputBytes) args.push("--max-output-bytes", inputs.maxOutputBytes);
  if (inputs.sarif) args.push("--report", inputs.sarif);
  if (inputs.changedOnly && inputs.base) args.push("--changed", inputs.base);
  return args;
}

export function actionFailureMessage(exitCode: number, stderr: string): string {
  if (exitCode === 1) return "quality found diagnostics at or above the configured failure level";
  const detail = stderr
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .find((line) => line.length > 0);
  return detail
    ? `quality could not complete (exit ${exitCode}): ${detail}`
    : `quality could not complete (exit ${exitCode})`;
}

export function resolveProjectPath(workspace: string, requested: string): string {
  return path.resolve(workspace, requested);
}

export interface SarifReportSummary {
  tools: number;
  findings: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isUnknownArray(value: unknown): value is readonly unknown[] {
  return Array.isArray(value);
}

function malformedReportError(detail: string): Error {
  return new Error(`quality SARIF report is malformed: ${detail}`);
}

/**
 * A `quality` CLI from before the `qualityExecuted` marker existed omits
 * that property, so its runs always count toward `tools`. Only an
 * explicit `qualityExecuted: false` (a tool whose process never started)
 * excludes a run from that count; its diagnostics still count toward
 * `findings`, since a required missing tool must stay visible.
 */
export function summarizeSarifReport(report: unknown): SarifReportSummary {
  if (!isRecord(report)) {
    throw malformedReportError("top-level value is not an object");
  }
  const runs = report["runs"];
  if (runs === undefined) return { tools: 0, findings: 0 };
  if (!isUnknownArray(runs)) {
    throw malformedReportError('"runs" is not an array');
  }

  let tools = 0;
  let findings = 0;
  for (const [index, run] of runs.entries()) {
    if (!isRecord(run)) {
      throw malformedReportError(`runs[${index}] is not an object`);
    }
    const results = run["results"];
    if (results !== undefined) {
      if (!isUnknownArray(results)) {
        throw malformedReportError(`runs[${index}].results is not an array`);
      }
      findings += results.length;
    }
    if (runQualityExecuted(run, index) !== false) tools += 1;
  }
  return { tools, findings };
}

function runQualityExecuted(run: Record<string, unknown>, index: number): boolean | undefined {
  const properties = run["properties"];
  if (properties === undefined) return undefined;
  if (!isRecord(properties)) {
    throw malformedReportError(`runs[${index}].properties is not an object`);
  }
  const executed = properties["qualityExecuted"];
  if (executed === undefined) return undefined;
  if (typeof executed !== "boolean") {
    throw malformedReportError(`runs[${index}].properties.qualityExecuted is not a boolean`);
  }
  return executed;
}
