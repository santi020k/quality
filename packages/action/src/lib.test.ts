import { describe, expect, it } from "vitest";
import {
  checkArguments,
  checksumFromFile,
  releaseRepository,
  releaseTarget,
} from "./lib.js";

describe("releaseTarget", () => {
  it("maps supported runners", () => {
    expect(releaseTarget("darwin", "arm64")).toBe("aarch64-apple-darwin");
    expect(releaseTarget("linux", "x64")).toBe("x86_64-unknown-linux-gnu");
    expect(releaseTarget("linux", "arm64")).toBe("aarch64-unknown-linux-gnu");
    expect(releaseTarget("win32", "x64")).toBe("x86_64-pc-windows-msvc");
  });

  it("rejects unsupported runners", () => {
    expect(() => releaseTarget("freebsd", "x64")).toThrow("does not publish");
  });
});

describe("releaseRepository", () => {
  it("prefers the Action repository and supports local workflow checks", () => {
    expect(releaseRepository("owner/action", "owner/workflow")).toBe("owner/action");
    expect(releaseRepository(undefined, "owner/workflow")).toBe("owner/workflow");
  });

  it("rejects missing repository context", () => {
    expect(() => releaseRepository(undefined, undefined)).toThrow("repository is unavailable");
  });
});

it("extracts the checksum for the requested asset", () => {
  const hash = "a".repeat(64);
  expect(checksumFromFile(`${hash}  quality-linux.tar.gz\n`, "quality-linux.tar.gz")).toBe(hash);
});

it("builds a changed-file check without shell interpolation", () => {
  expect(
    checkArguments({
      base: "origin/main",
      changedOnly: true,
      reportLevel: "warning",
      failLevel: "error",
      sarif: "/tmp/quality.sarif",
    }),
  ).toEqual([
    "check",
    "--format",
    "github",
    "--report-level",
    "warning",
    "--fail-level",
    "error",
    "--report",
    "/tmp/quality.sarif",
    "--changed",
    "origin/main",
  ]);
});
