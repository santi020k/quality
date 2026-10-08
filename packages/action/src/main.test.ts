import path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getExecOutput: vi.fn<() => Promise<{ exitCode: number; stdout: string; stderr: string }>>(),
  readFile: vi.fn<() => Promise<string>>(),
  setFailed: vi.fn<(message: string | Error) => void>(),
  setOutput: vi.fn<(name: string, value: unknown) => void>(),
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
vi.mock("@actions/tool-cache", () => ({ find: () => "/cached-quality" }));
vi.mock("node:fs/promises", () => ({
  chmod: vi.fn<() => Promise<void>>(),
  readFile: mocks.readFile,
}));

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  vi.stubEnv("GITHUB_WORKSPACE", "/workspace");
  vi.stubEnv("GITHUB_BASE_REF", "");
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
