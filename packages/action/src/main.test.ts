import { createHash } from "node:crypto";
import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cacheToolName, releaseBaseUrl, releaseTarget } from "./lib.js";

const mocks = vi.hoisted(() => ({
  getExecOutput: vi.fn<() => Promise<{ exitCode: number; stdout: string; stderr: string }>>(),
  readFile: vi.fn<(file: string, encoding?: string) => Promise<string | Buffer>>(),
  chmod: vi.fn<() => Promise<void>>(),
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
    return inputs[name] ?? "";
  },
  getBooleanInput: (name: string): boolean => name === "require-checks",
  info: vi.fn<(message: string) => void>(),
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
  readFile: mocks.readFile,
}));

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  mocks.find.mockReturnValue("/cached-quality");
  vi.stubEnv("GITHUB_WORKSPACE", "/workspace");
  vi.stubEnv("GITHUB_BASE_REF", "");
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/quality");
  vi.stubEnv("QUALITY_ACTION_REPOSITORY", "");
  vi.stubEnv("QUALITY_RELEASE_BASE_URL", "");
});

afterEach(() => {
  vi.unstubAllEnvs();
});

it.each([2, 127])("preserves an operational CLI failure (exit %i) without reading SARIF", async (exitCode) => {
  mocks.getExecOutput.mockResolvedValue({
    exitCode,
    stdout: "",
    stderr: "quality: invalid configuration\n",
  });
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
  mocks.getExecOutput.mockResolvedValue({ exitCode, stdout: "", stderr: "" });
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
      "quality found diagnostics at or above the configured failure level",
    );
  } else {
    expect(mocks.setFailed).not.toHaveBeenCalled();
  }
});

it("never shares a tool-cache namespace between different release repositories or base URLs", async () => {
  mocks.getExecOutput.mockResolvedValue({ exitCode: 0, stdout: "", stderr: "" });
  mocks.readFile.mockResolvedValue(JSON.stringify({ runs: [] }));

  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-a");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.find).toHaveBeenCalled());
  const toolNameForRepoA = mocks.find.mock.calls[0]?.[0];

  vi.resetModules();
  mocks.find.mockClear();
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-b");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.find).toHaveBeenCalled());
  const toolNameForRepoB = mocks.find.mock.calls[0]?.[0];

  vi.resetModules();
  mocks.find.mockClear();
  vi.stubEnv("GITHUB_ACTION_REPOSITORY", "acme/repo-a");
  vi.stubEnv("QUALITY_RELEASE_BASE_URL", "https://fixtures.example.com");
  await import("./main.js");
  await vi.waitFor(() => expect(mocks.find).toHaveBeenCalled());
  const toolNameForOverrideBaseUrl = mocks.find.mock.calls[0]?.[0];

  expect(toolNameForRepoA).toBe(cacheToolName(releaseBaseUrl("acme/repo-a", undefined)));
  expect(toolNameForRepoB).toBe(cacheToolName(releaseBaseUrl("acme/repo-b", undefined)));
  expect(toolNameForOverrideBaseUrl).toBe(cacheToolName("https://fixtures.example.com"));
  expect(new Set([toolNameForRepoA, toolNameForRepoB, toolNameForOverrideBaseUrl]).size).toBe(3);
});

it("resolves the release repository before the cache lookup, and inserts under the same key it looked up", async () => {
  mocks.getExecOutput.mockResolvedValue({ exitCode: 0, stdout: "", stderr: "" });
  mocks.find.mockReturnValue("");

  const target = releaseTarget(process.platform, process.arch);
  const windows = process.platform === "win32";
  const binaryName = windows ? "quality.exe" : "quality";
  const archivePath = "/downloads/archive";
  const checksumPath = "/downloads/archive.sha256";
  const extractedDirectory = "/extracted";
  const archiveBytes = Buffer.from("fake quality release archive");
  const expectedChecksum = createHash("sha256").update(archiveBytes).digest("hex");
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

  await import("./main.js");

  await vi.waitFor(() => expect(mocks.cacheFile).toHaveBeenCalled());

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
});
