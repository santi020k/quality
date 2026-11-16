import { createHash } from "node:crypto";
import type { getExecOutput } from "@actions/exec";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import actionPackage from "../package.json" with { type: "json" };
import { cacheToolName, releaseBaseUrl, releaseTarget } from "./lib.js";

const mocks = vi.hoisted(() => ({
  getExecOutput: vi.fn<typeof getExecOutput>(),
  checkResult: { exitCode: 0, stdout: "", stderr: "" },
  binaryVersion: "quality 1.0.0\n",
  inputs: new Map<string, string>(),
  changedOnly: false,
  readFile: vi.fn<(file: string, encoding?: string) => Promise<string | Buffer>>(),
  chmod: vi.fn<() => Promise<void>>(),
  mkdtemp: vi.fn<(prefix: string) => Promise<string>>(),
  rm: vi.fn<(directory: string, options: { recursive: boolean; force: boolean }) => Promise<void>>(),
  warning: vi.fn<(message: string) => void>(),
  setFailed: vi.fn<(message: string | Error) => void>(),
  setOutput: vi.fn<(name: string, value: unknown) => void>(),
  find: vi.fn<(tool: string, version: string, arch?: string) => string>(),
  downloadTool: vi.fn<(url: string) => Promise<string>>(),
  extractTar: vi.fn<(file: string) => Promise<string>>(),
  extractZip: vi.fn<(file: string) => Promise<string>>(),
  cacheFile: vi.fn<
    (source: string, target: string, tool: string, version: string, arch?: string) => Promise<string>
  >(),
}));

vi.mock("@actions/core", () => ({
  getInput: (name: string): string => {
    const inputs: Record<string, string> = {
      version: "v1.0.0",
      "working-directory": ".",
      "report-level": "warning",
      "fail-level": "warning",
      sarif: "quality.sarif",
    };
    return mocks.inputs.get(name) ?? inputs[name] ?? "";
  },
  getBooleanInput: (name: string): boolean => name === "require-checks" || (name === "changed-only" && mocks.changedOnly),
  info: vi.fn<(message: string) => void>(),
  warning: mocks.warning,
  setFailed: mocks.setFailed,
  setOutput: mocks.setOutput,
}));
vi.mock("@actions/exec", () => ({ getExecOutput: mocks.getExecOutput }));
vi.mock("@actions/tool-cache", () => ({
  find: mocks.find,
  downloadTool: mocks.downloadTool,
  extractTar: mocks.extractTar,
  extractZip: mocks.extractZip,
  cacheFile: mocks.cacheFile,
}));
vi.mock("node:fs/promises", () => ({
  chmod: mocks.chmod,
  mkdtemp: mocks.mkdtemp,
  readFile: mocks.readFile,
  rm: mocks.rm,
}));

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  mocks.find.mockReturnValue("/cached-quality");
  mocks.inputs.clear();
  mocks.changedOnly = false;
  mocks.binaryVersion = "quality 1.0.0\n";
  mocks.checkResult = { exitCode: 0, stdout: "", stderr: "" };
  mocks.getExecOutput.mockImplementation(async (_binary, args) => args?.[0] === "--version"
    ? { exitCode: 0, stdout: mocks.binaryVersion, stderr: "" }
    : mocks.checkResult);
  mocks.mkdtemp.mockResolvedValue("/runner-temp/quality-action-unique");
  mocks.rm.mockResolvedValue();
  mocks.readFile.mockResolvedValue(JSON.stringify({ runs: [] }));
  vi.stubEnv("GITHUB_WORKSPACE", "/workspace");
  vi.stubEnv("GITHUB_BASE_REF", "");
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/quality");
  vi.stubEnv("QUALITY_ACTION_REPOSITORY", "");
  vi.stubEnv("QUALITY_RELEASE_BASE_URL", "");
  vi.stubEnv("RUNNER_TEMP", "/runner-temp");
});

afterEach(() => {
  vi.unstubAllEnvs();
});

it.each([2, 127])("preserves an operational CLI failure (exit %i) without reading SARIF", async (exitCode) => {
  mocks.checkResult = {
    exitCode,
    stdout: "",
    stderr: "quality: invalid configuration\n",
  };
  mocks.readFile.mockRejectedValue(new Error("ENOENT: quality.sarif"));

  await import("./main.js");

  await vi.waitFor(() => {
    expect(mocks.setFailed).toHaveBeenCalledWith(
      `quality could not complete (exit ${exitCode}): quality: invalid configuration`,
    );
  });
  expect(mocks.readFile).not.toHaveBeenCalled();
  expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number));
  expect(mocks.setOutput).not.toHaveBeenCalledWith("sarif", expect.anything());
  expect(mocks.setOutput).not.toHaveBeenCalledWith("findings", expect.anything());
  expect(mocks.setOutput).not.toHaveBeenCalledWith("tools", expect.anything());
});

it.each([0, 1])("publishes SARIF outputs for a completed check (exit %i)", async (exitCode) => {
  mocks.checkResult = { exitCode, stdout: "", stderr: "" };
  mocks.readFile.mockResolvedValue(JSON.stringify({
    runs: [{ results: [{ message: { text: "Example diagnostic" } }] }, { results: [] }],
  }));

  await import("./main.js");

  await vi.waitFor(() => {
    expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number));
  });
  expect(mocks.readFile).toHaveBeenCalledWith(path.resolve("/workspace", "quality.sarif"), "utf8");
  expect(mocks.setOutput).toHaveBeenCalledWith("sarif", path.resolve("/workspace", "quality.sarif"));
  expect(mocks.setOutput).toHaveBeenCalledWith("findings", 1);
  expect(mocks.setOutput).toHaveBeenCalledWith("tools", 2);
  if (exitCode === 1) {
    expect(mocks.setFailed).toHaveBeenCalledWith(
      "quality checks failed; inspect the reported diagnostics and tool failures",
    );
  } else {
    expect(mocks.setFailed).not.toHaveBeenCalled();
  }
});

it("never shares a tool-cache namespace between different release repositories or base URLs", async () => {
  mocks.readFile.mockResolvedValue(JSON.stringify({ runs: [] }));

  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-a");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  const toolNameForRepoA = mocks.find.mock.calls[0]?.[0];

  vi.resetModules();
  mocks.find.mockClear();
  mocks.setOutput.mockClear();
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-b");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  const toolNameForRepoB = mocks.find.mock.calls[0]?.[0];

  vi.resetModules();
  mocks.find.mockClear();
  mocks.setOutput.mockClear();
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-a");
  vi.stubEnv("QUALITY_RELEASE_BASE_URL", "https://fixtures.example.com");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  const toolNameForOverrideBaseUrl = mocks.find.mock.calls[0]?.[0];

  expect(toolNameForRepoA).toBe(cacheToolName(releaseBaseUrl("acme/repo-a", undefined)));
  expect(toolNameForRepoB).toBe(cacheToolName(releaseBaseUrl("acme/repo-b", undefined)));
  expect(toolNameForOverrideBaseUrl).toBe(cacheToolName("https://fixtures.example.com"));
  expect(new Set([toolNameForRepoA, toolNameForRepoB, toolNameForOverrideBaseUrl]).size).toBe(3);
});

function prepareDownload(checksumMatches = true): { target: string; binaryName: string; asset: string; extractedDirectory: string } {
  mocks.find.mockReturnValue("");
  const target = releaseTarget(process.platform, process.arch);
  const windows = process.platform === "win32";
  const binaryName = windows ? "quality.exe" : "quality";
  const archivePath = "/downloads/archive";
  const checksumPath = "/downloads/archive.sha256";
  const extractedDirectory = "/extracted";
  const archiveBytes = Buffer.from("fake quality release archive");
  const expectedChecksum = checksumMatches ? createHash("sha256").update(archiveBytes).digest("hex") : "0".repeat(64);
  const asset = `quality-${target}.${windows ? "zip" : "tar.gz"}`;
  const sarifPath = path.resolve("/workspace", "quality.sarif");

  mocks.downloadTool.mockImplementation(async (url: string) =>
    url.endsWith(".sha256") ? checksumPath : archivePath,
  );
  mocks.readFile.mockImplementation(async (file: string) => {
    if (file === checksumPath) return `${expectedChecksum}  ${asset}\n`;
    if (file === archivePath) return archiveBytes;
    if (file === sarifPath) return JSON.stringify({ runs: [] });
    throw new Error(`unexpected readFile(${file})`);
  });
  mocks.extractTar.mockResolvedValue(extractedDirectory);
  mocks.extractZip.mockResolvedValue(extractedDirectory);
  mocks.cacheFile.mockResolvedValue("/cached-after-install");
  return { target, binaryName, asset, extractedDirectory };
}

it("resolves the release repository before the cache lookup, and inserts under the same key it looked up", async () => {
  const { target, binaryName, extractedDirectory } = prepareDownload();

  await import("./main.js");

  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));

  const toolNameUsedToFind = mocks.find.mock.calls[0]?.[0];
  const toolNameUsedToCache = mocks.cacheFile.mock.calls[0]?.[2];
  expect(toolNameUsedToFind).toBe(cacheToolName(releaseBaseUrl("acme/quality", undefined)));
  expect(toolNameUsedToCache).toBe(toolNameUsedToFind);
  expect(mocks.cacheFile).toHaveBeenCalledWith(
    path.join(extractedDirectory, binaryName),
    binaryName,
    toolNameUsedToFind,
    "v1.0.0",
    target,
  );
  expect(mocks.getExecOutput).toHaveBeenNthCalledWith(1,
    path.join(extractedDirectory, binaryName), ["--version"], { silent: true, ignoreReturnCode: true });
  expect(mocks.setFailed).not.toHaveBeenCalled();
});

it("defaults to the version bundled with the pinned Action", async () => {
  mocks.inputs.set("version", "");
  mocks.binaryVersion = `quality ${actionPackage.version}\n`;
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.find).toHaveBeenCalledWith(expect.any(String), `v${actionPackage.version}`, releaseTarget(process.platform, process.arch));
  expect(mocks.setFailed).not.toHaveBeenCalled();
});

it("uses the latest endpoint only when requested and avoids a mutable latest cache", async () => {
  const { asset } = prepareDownload();
  mocks.inputs.set("version", "latest");
  mocks.binaryVersion = "quality 2.0.0\n";
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.downloadTool).toHaveBeenCalledWith(`https://github.com/acme/quality/releases/latest/download/${asset}`);
  expect(mocks.find).not.toHaveBeenCalled();
  expect(mocks.cacheFile).not.toHaveBeenCalled();
  expect(mocks.setFailed).not.toHaveBeenCalled();
});

it.each(["v2", "../other", "--dry-run"])("rejects invalid version %s before installation", async (version) => {
  mocks.inputs.set("version", version);
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledWith(expect.stringContaining("exact semantic version"));
  expect(mocks.find).not.toHaveBeenCalled();
  expect(mocks.downloadTool).not.toHaveBeenCalled();
  expect(mocks.getExecOutput).not.toHaveBeenCalled();
});

it("never aliases distinct release build metadata through the tool cache", async () => {
  for (const version of ["2.0.0+build.one", "2.0.0+build.two"]) {
    vi.resetModules();
    mocks.setOutput.mockClear();
    const { asset } = prepareDownload();
    mocks.find.mockReturnValue("/cached-other-build");
    mocks.inputs.set("version", version);
    mocks.binaryVersion = `quality ${version}\n`;
    await import("./main.js");
    await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
    expect(mocks.downloadTool).toHaveBeenCalledWith(`https://github.com/acme/quality/releases/download/v${version}/${asset}`);
    expect(mocks.find).not.toHaveBeenCalled();
    expect(mocks.cacheFile).not.toHaveBeenCalled();
    expect(mocks.setFailed).not.toHaveBeenCalled();
  }
});

it.each(["cache", "download"])("rejects a mismatched %s binary before running checks", async (source) => {
  if (source === "download") prepareDownload();
  mocks.binaryVersion = "quality 9.0.0\n";
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledWith("installed quality version 9.0.0 does not match requested v1.0.0");
  expect(mocks.getExecOutput).toHaveBeenCalledTimes(1);
  expect(mocks.getExecOutput.mock.calls[0]?.[1]).toEqual(["--version"]);
  expect(mocks.cacheFile).not.toHaveBeenCalled();
});

it("rejects a checksum mismatch before extracting or executing a download", async () => {
  prepareDownload(false);
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledWith(expect.stringContaining("checksum mismatch"));
  expect(mocks.extractTar).not.toHaveBeenCalled();
  expect(mocks.extractZip).not.toHaveBeenCalled();
  expect(mocks.getExecOutput).not.toHaveBeenCalled();
  expect(mocks.cacheFile).not.toHaveBeenCalled();
});

it("rejects option-like base values before running checks", async () => {
  mocks.inputs.set("base", "--dry-run");
  mocks.changedOnly = true;
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledWith(expect.stringContaining("base must be a Git revision"));
  expect(mocks.getExecOutput).not.toHaveBeenCalled();
});

it.each([0, 1])("reports accurate counts with public SARIF disabled (exit %i)", async (exitCode) => {
  mocks.inputs.set("sarif", "");
  mocks.checkResult.exitCode = exitCode;
  mocks.readFile.mockResolvedValue(JSON.stringify({ runs: [
    { results: [{}], properties: { qualityExecuted: false } },
    { results: [{}], properties: { qualityExecuted: true } },
  ] }));
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  const temporaryReport = path.join("/runner-temp/quality-action-unique", "quality.sarif");
  expect(mocks.mkdtemp).toHaveBeenCalledWith(path.join("/runner-temp", "quality-action-"));
  expect(mocks.readFile).toHaveBeenCalledWith(temporaryReport, "utf8");
  expect(mocks.getExecOutput.mock.calls[1]?.[1]).toEqual(expect.arrayContaining(["--report", temporaryReport]));
  expect(mocks.setOutput).toHaveBeenCalledWith("findings", 2);
  expect(mocks.setOutput).toHaveBeenCalledWith("tools", 1);
  expect(mocks.setOutput).not.toHaveBeenCalledWith("sarif", expect.anything());
  expect(mocks.rm).toHaveBeenCalledWith("/runner-temp/quality-action-unique", { recursive: true, force: true });
});

it("cleans its private report and reports duration after a malformed report", async () => {
  mocks.inputs.set("sarif", "");
  mocks.readFile.mockResolvedValue('{"runs":"invalid"}');
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledWith(expect.stringContaining("SARIF report is malformed"));
  expect(mocks.rm).toHaveBeenCalledWith("/runner-temp/quality-action-unique", { recursive: true, force: true });
});

it("preserves the original failure if temporary report cleanup also fails", async () => {
  mocks.inputs.set("sarif", "");
  mocks.getExecOutput.mockRejectedValue(new Error("binary cannot start"));
  mocks.rm.mockRejectedValue(new Error("cleanup failed"));
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.setOutput).toHaveBeenCalledWith("duration-ms", expect.any(Number)));
  expect(mocks.setFailed).toHaveBeenCalledExactlyOnceWith("binary cannot start");
  expect(mocks.warning).toHaveBeenCalledWith(expect.stringContaining("temporary quality report"));
});
