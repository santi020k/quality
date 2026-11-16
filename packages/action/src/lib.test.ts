import { describe, expect, it } from "vitest";
import {
  cacheToolName,
  checkArguments,
  actionFailureMessage,
  checksumFromFile,
  releaseRepository,
  releaseBaseUrl,
  releaseTarget,
  releaseVersion,
  summarizeSarifReport,
  verifyBinaryVersion,
} from "./lib.js";

describe("releaseVersion", () => {
  it.each(["2.0.0", "v2.0.0", "2.0.0-rc.1+build.123"])("accepts an exact version %s", (version) => {
    expect(releaseVersion(version, "3.0.0")).toBe(version.startsWith("v") ? version : `v${version}`);
  });

  it("uses the Action package version by default and requires explicit latest", () => {
    expect(releaseVersion("", "2.0.0")).toBe("v2.0.0");
    expect(releaseVersion("latest", "2.0.0")).toBe("latest");
  });

  it.each(["v2", "main", "v2.0", "^2.0.0", "../v2.0.0", "--version", "1.02.3", "1.2.3-01", "1.2.3\n", "1.2.3/other"])(
    "rejects unsupported version %j",
    (version) => expect(() => releaseVersion(version, "2.0.0")).toThrow("exact semantic version"),
  );
});

describe("verifyBinaryVersion", () => {
  it("checks exact binaries and accepts a valid version for an explicit latest request", () => {
    expect(verifyBinaryVersion("quality 2.0.0\n", "v2.0.0")).toBe("2.0.0");
    expect(verifyBinaryVersion("quality 3.0.1-rc.1+build.123\r\n", "latest")).toBe("3.0.1-rc.1+build.123");
  });

  it("rejects mismatched and malformed binary versions", () => {
    expect(() => verifyBinaryVersion("quality 1.9.0", "v2.0.0")).toThrow("does not match requested");
    expect(() => verifyBinaryVersion("another 2.0.0", "v2.0.0")).toThrow("did not report a valid version");
    expect(() => verifyBinaryVersion("quality 2.0.0\nextra", "latest")).toThrow("did not report a valid version");
  });
});

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
    "--changed=origin/main",
  ]);
});

it("distinguishes unsuccessful checks from operational CLI failures", () => {
  expect(actionFailureMessage(1, "")).toContain("checks failed");
  expect(actionFailureMessage(2, "quality: invalid configuration\n")).toBe(
    "quality could not complete (exit 2): quality: invalid configuration",
  );
});

it("rejects option-like base inputs without restricting Git revision expressions", () => {
  const inputs = {
    base: "--dry-run",
    changedOnly: true,
    reportLevel: "warning",
    failLevel: "error",
    sarif: "",
    requireChecks: false,
    jobs: "",
    timeoutSeconds: "",
    maxOutputBytes: "",
  };
  expect(() => checkArguments(inputs)).toThrow("base must be a Git revision");
  expect(() => checkArguments({ ...inputs, base: "HEAD\n--dry-run" })).toThrow("base must be a Git revision");
  for (const base of ["HEAD~1", "origin/main^{commit}", "HEAD@{1}"]) {
    expect(checkArguments({ ...inputs, base })).toContain(`--changed=${base}`);
  }
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
