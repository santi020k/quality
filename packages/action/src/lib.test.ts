import { describe, expect, it } from "vitest";
import {
  cacheToolName,
  checkArguments,
  actionFailureMessage,
  checksumFromFile,
  releaseRepository,
  releaseBaseUrl,
  releaseTarget,
  summarizeSarifReport,
} from "./lib.js";

it("uses GitHub releases by default and normalizes a release base override", () => {
  expect(releaseBaseUrl("acme/quality", undefined)).toBe("https://github.com/acme/quality");
  expect(releaseBaseUrl("acme/quality", "http://127.0.0.1:8080///")).toBe(
    "http://127.0.0.1:8080",
  );
});

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
      requireChecks: true,
      jobs: "2",
      timeoutSeconds: "30",
      maxOutputBytes: "4096",
    }),
  ).toEqual([
    "check",
    "--format",
    "github",
    "--report-level",
    "warning",
    "--fail-level",
    "error",
    "--require-checks",
    "--jobs",
    "2",
    "--timeout-seconds",
    "30",
    "--max-output-bytes",
    "4096",
    "--report",
    "/tmp/quality.sarif",
    "--changed",
    "origin/main",
  ]);
});

it("distinguishes diagnostics from operational CLI failures", () => {
  expect(actionFailureMessage(1, "")).toContain("found diagnostics");
  expect(actionFailureMessage(2, "quality: invalid configuration\n")).toBe(
    "quality could not complete (exit 2): quality: invalid configuration",
  );
});

describe("cacheToolName", () => {
  it("is deterministic, fixed-length, and path-safe", () => {
    const name = cacheToolName("https://github.com/acme/quality");
    expect(name).toBe(cacheToolName("https://github.com/acme/quality"));
    expect(name).toMatch(/^quality-[0-9a-f]{64}$/u);
  });

  it("never shares a cache namespace between different release sources", () => {
    expect(cacheToolName("https://github.com/acme/quality")).not.toBe(
      cacheToolName("https://github.com/acme/quality-fork"),
    );
    expect(cacheToolName("https://github.com/acme/quality")).not.toBe(
      cacheToolName("http://127.0.0.1:8080"),
    );
  });
});

describe("summarizeSarifReport", () => {
  it("counts findings across every run, including ones excluded from the tool count", () => {
    const summary = summarizeSarifReport({
      runs: [
        { results: [{}, {}], properties: { qualityExecuted: false } },
        { results: [{}] },
      ],
    });
    expect(summary).toEqual({ tools: 1, findings: 3 });
  });

  it("counts a run without a quality-executed marker, for reports from older CLI versions", () => {
    expect(summarizeSarifReport({ runs: [{ results: [] }] })).toEqual({ tools: 1, findings: 0 });
  });

  it("counts a run that executed successfully", () => {
    expect(
      summarizeSarifReport({ runs: [{ results: [], properties: { qualityExecuted: true } }] }),
    ).toEqual({ tools: 1, findings: 0 });
  });

  it("excludes only a run explicitly marked as not executed, keeping its findings", () => {
    const summary = summarizeSarifReport({
      runs: [
        { results: [{}], properties: { qualityExecuted: false } },
        { results: [{}], properties: { qualityExecuted: true } },
      ],
    });
    expect(summary).toEqual({ tools: 1, findings: 2 });
  });

  it("treats an absent runs array as an empty report", () => {
    expect(summarizeSarifReport({})).toEqual({ tools: 0, findings: 0 });
  });

  const malformedReports: ReadonlyArray<readonly [unknown, string]> = [
    [null, "top-level value is not an object"],
    [[], "top-level value is not an object"],
    [{ runs: "not-an-array" }, '"runs" is not an array'],
    [{ runs: [null] }, "runs[0] is not an object"],
    [{ runs: [[]] }, "runs[0] is not an object"],
    [{ runs: [{ results: {} }] }, "runs[0].results is not an array"],
    [{ runs: [{ properties: "not-an-object" }] }, "runs[0].properties is not an object"],
    [{ runs: [{ properties: [] }] }, "runs[0].properties is not an object"],
    [
      { runs: [{ properties: { qualityExecuted: "false" } }] },
      "runs[0].properties.qualityExecuted is not a boolean",
    ],
  ];
  it.each(malformedReports)("rejects a malformed report (%#)", (report, detail) => {
    expect(() => summarizeSarifReport(report)).toThrow(detail);
  });
});
