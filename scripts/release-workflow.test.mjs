import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { prepareReleaseNotes } from "./prepare-release-notes.mjs";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const notesScript = resolve("scripts/prepare-release-notes.mjs");

function job(name) {
  const section = workflow.split(/(?=^  [a-z][a-z-]*:\n)/m)
    .find((part) => part.startsWith(`  ${name}:\n`));
  assert.ok(section, `Missing ${name} job`);
  return section;
}

function runStep(jobName, stepName) {
  const section = job(jobName).split(/(?=^      - )/m)
    .find((part) => part.includes(`name: ${stepName}\n`));
  assert.ok(section, `Missing ${stepName} step`);
  const run = section.split("        run: |\n")[1];
  assert.ok(run, `Missing shell script for ${stepName}`);
  return run.split("\n").filter((line) => line.startsWith("          "))
    .map((line) => line.slice(10)).join("\n");
}

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), "quality-release-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const bin = join(directory, "bin");
  mkdirSync(bin);
  const git = (...args) => execFileSync("git", args, { cwd: directory, encoding: "utf8" }).trim();
  git("init", "--quiet");
  git("-c", "user.name=Release test", "-c", "user.email=release@example.invalid",
    "commit", "--quiet", "--allow-empty", "-m", "Fixture");
  const sha = git("rev-parse", "HEAD");
  const callsPath = join(directory, "gh-calls.jsonl");
  const outputPath = join(directory, "outputs");
  writeFileSync(callsPath, "");
  writeFileSync(outputPath, "");
  writeFileSync(join(bin, "cargo"), '#!/bin/sh\nprintf \'%s\\n\' "$FAKE_CARGO_METADATA"\n', { mode: 0o755 });
  writeFileSync(join(bin, "gh"), `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.GH_CALLS, JSON.stringify(args) + "\\n");
function reply(data, status = 200) {
  if (args.includes("--include")) {
    console.log("HTTP/2.0 " + status + " Test\\r\\nContent-Type: application/json\\r\\n\\r\\n" + JSON.stringify(data));
  } else if (args.includes("--jq")) {
    console.log(data.sha);
  }
  process.exit(status >= 400 ? 1 : 0);
}
if (args[0] === "release" && args[1] === "view") {
  process.exit(process.env.FAKE_RELEASE_EXISTS === "true" ? 0 : 1);
}
const endpoint = args.find((arg) => arg.startsWith("repos/")) || "";
if (args[0] === "api" && !args.includes("--method") && process.env.FAKE_API_FAILURE && endpoint.includes(process.env.FAKE_API_FAILURE)) {
  reply({ message: "Unavailable" }, Number(process.env.FAKE_API_STATUS || "503"));
}
if (args[0] === "api" && endpoint.includes("/commits/")) {
  const ref = decodeURIComponent(endpoint.split("/commits/")[1]);
  const major = "v" + process.env.VERSION.split(".")[0];
  const sha = ref === "refs/tags/" + process.env.TAG ? process.env.FAKE_TAG_SHA
    : ref === "refs/tags/" + major ? process.env.FAKE_MAJOR_SHA
    : ref === "refs/tags/" + process.env.FAKE_LATEST_TAG ? process.env.FAKE_LATEST_SHA : undefined;
  reply(sha ? { sha } : { message: "Not Found" }, sha ? 200 : 404);
} else if (args[0] === "api" && endpoint.includes("/git/ref/tags/")) {
  const isVersion = endpoint.endsWith("/" + process.env.TAG);
  const exists = isVersion ? process.env.FAKE_TAG_EXISTS : process.env.FAKE_MAJOR_EXISTS;
  reply({ object: { sha: process.env.FAKE_MAJOR_SHA } }, exists === "true" ? 200 : 404);
} else if (args[0] === "api" && endpoint.includes("/contents/")) {
  reply({ encoding: "base64", content: Buffer.from(JSON.stringify({ version: process.env.FAKE_MAJOR_VERSION })).toString("base64") });
} else if (args[0] === "api" && endpoint.endsWith("/releases/latest")) {
  reply({ tag_name: process.env.FAKE_LATEST_TAG }, process.env.FAKE_LATEST_EXISTS === "false" ? 404 : 200);
}
`, { mode: 0o755 });
  const env = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH}`,
    GITHUB_REPOSITORY: "fixture/quality",
    GITHUB_SHA: sha,
    GITHUB_OUTPUT: outputPath,
    GH_CALLS: callsPath,
    TAG: "v2.0.0",
    VERSION: "2.0.0",
    PRERELEASE: "false",
    FAKE_CARGO_METADATA: JSON.stringify({ packages: [{ name: "quality-cli", version: "2.0.0" }] }),
    FAKE_TAG_SHA: sha,
    FAKE_TAG_EXISTS: "true",
    FAKE_MAJOR_EXISTS: "true",
    FAKE_MAJOR_SHA: "a".repeat(40),
    FAKE_MAJOR_VERSION: "2.0.0",
    FAKE_LATEST_EXISTS: "true",
    FAKE_LATEST_TAG: "v1.9.0",
    FAKE_LATEST_SHA: "c".repeat(40),
    FAKE_API_FAILURE: "",
    FAKE_RELEASE_EXISTS: "false",
  };
  return {
    directory, env, git, sha,
    run(jobName, stepName, overrides = {}) {
      return spawnSync("bash", ["--noprofile", "--norc", "-e", "-o", "pipefail", "-c", runStep(jobName, stepName)], {
        cwd: directory, env: { ...env, ...overrides }, encoding: "utf8",
      });
    },
    calls() {
      return readFileSync(callsPath, "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
    },
    outputs() {
      return Object.fromEntries(readFileSync(outputPath, "utf8").trim().split("\n").map((line) => line.split("=")));
    },
  };
}

function assertSuccess(result) {
  assert.equal(result.status, 0, result.error?.message ?? `${result.stdout}\n${result.stderr}`);
}

test("prepared notes retain migration guidance and select only the exact version", () => {
  const changelog = "# Changelog\n\n## Unreleased\n\nFuture.\n\n## 2.0.0\n\n### Major Changes\n\n- Migrate old config.\n\n## 2.0.0-rc.1\n\nPreview only.\n\n## 1.4.0\n\nOld.\n";
  assert.equal(prepareReleaseNotes(changelog, "2.0.0"), "### Major Changes\n\n- Migrate old config.\n");
  assert.equal(prepareReleaseNotes(changelog, "2.0.0-rc.1"), "Preview only.\n");
  assert.throws(() => prepareReleaseNotes(changelog, "3.0.0"), /no prepared release notes/);
  assert.throws(() => prepareReleaseNotes("## 2.0.0\n\n## 1.4.0\nOld.", "2.0.0"), /no prepared release notes/);
});

test("notes CLI writes the prepared section and fails without creating missing notes", (t) => {
  const f = fixture(t);
  writeFileSync(join(f.directory, "CHANGELOG.md"), "## 2.0.0\n\nMigration instructions.\n");
  const output = join(f.directory, "release-notes.md");
  assertSuccess(spawnSync(process.execPath, [notesScript, "2.0.0", output], { cwd: f.directory, encoding: "utf8" }));
  assert.equal(readFileSync(output, "utf8"), "Migration instructions.\n");
  const failure = spawnSync(process.execPath, [notesScript, "3.0.0", output], { cwd: f.directory, encoding: "utf8" });
  assert.equal(failure.status, 1);
  assert.match(failure.stderr, /no prepared release notes for 3.0.0/);
  assert.equal(readFileSync(output, "utf8"), "Migration instructions.\n");
});

for (const [version, prerelease] of [
  ["2.0.0", "false"],
  ["2.0.0-rc.1", "true"],
  ["2.0.0+ci-build", "false"],
  ["2.0.0-rc.1+ci-build", "true"],
]) {
  test(`release preparation classifies ${version} correctly`, (t) => {
    const f = fixture(t);
    assertSuccess(f.run("prepare", "Resolve version", {
      FAKE_CARGO_METADATA: JSON.stringify({ packages: [{ name: "quality-cli", version }] }),
    }));
    assert.deepEqual(f.outputs(), { version, tag: `v${version}`, prerelease, release: "true" });
  });
}

for (const annotated of [false, true]) {
  test(`preparation accepts matching ${annotated ? "annotated" : "lightweight"} tags and rejects a different commit`, (t) => {
    const f = fixture(t);
    f.git("-c", "user.name=Release test", "-c", "user.email=release@example.invalid", "tag",
      ...(annotated ? ["-a", "-m", "Prepared release"] : []), f.env.TAG);
    assertSuccess(f.run("prepare", "Resolve version"));
    f.git("-c", "user.name=Release test", "-c", "user.email=release@example.invalid",
      "commit", "--quiet", "--allow-empty", "-m", "Other commit");
    const failure = f.run("prepare", "Resolve version", {
      GITHUB_SHA: f.git("rev-parse", "HEAD"), FAKE_RELEASE_EXISTS: "true",
    });
    assert.equal(failure.status, 1);
    assert.match(failure.stdout, /Existing tag .* does not point to the release commit/);
  });
}

test("publication creates an absent tag at the intended commit and never moves an existing wrong tag", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("publish", "Ensure version tag matches release commit", { FAKE_TAG_EXISTS: "false" }));
  assert.ok(f.calls().some((args) => args.includes("POST") && args.includes(`sha=${f.sha}`) && args.includes("ref=refs/tags/v2.0.0")));
  const before = f.calls().length;
  const failure = f.run("publish", "Ensure version tag matches release commit", { FAKE_TAG_SHA: "b".repeat(40) });
  assert.equal(failure.status, 1);
  assert.match(failure.stdout, /does not point to the release commit/);
  assert.ok(f.calls().slice(before).every((args) => !args.includes("POST") && !args.includes("PATCH")));
});

test("an existing release at the intended commit remains eligible for smoke on push reruns", (t) => {
  const f = fixture(t);
  f.git("tag", f.env.TAG);
  assertSuccess(f.run("prepare", "Resolve version", { FAKE_RELEASE_EXISTS: "true" }));
  assert.equal(f.outputs().release, "false");
  assert.equal(f.outputs().version, "2.0.0");
  const smoke = job("action-smoke");
  assert.match(smoke, /needs\.prepare\.outputs\.release == 'false' && needs\.prepare\.outputs\.version != '0\.0\.0'/);
  assert.doesNotMatch(smoke, /github\.event_name/);
  assert.match(smoke, /needs\.prepare\.result == 'success'/);
});

for (const prerelease of ["false", "true"]) {
  test(`publication keeps latest unchanged and carries notes with prerelease=${prerelease}`, (t) => {
    const f = fixture(t);
    mkdirSync(join(f.directory, "release-notes"));
    mkdirSync(join(f.directory, "release"));
    writeFileSync(join(f.directory, "release-notes/release-notes.md"), "Migration guidance.\n");
    writeFileSync(join(f.directory, "release/archive.tar.gz"), "fixture");
    assertSuccess(f.run("publish", "Create release", { PRERELEASE: prerelease }));
    const [args] = f.calls();
    assert.ok(args.includes("--latest=false"));
    assert.ok(args.includes(`--prerelease=${prerelease}`));
    assert.ok(args.includes("--verify-tag"));
    assert.equal(args[args.indexOf("--notes-file") + 1], "release-notes/release-notes.md");
    assert.ok(!args.includes("--generate-notes"));
  });
}

test("published smoke verifies both the remote tag and the Action checkout", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("action-smoke", "Verify published release commit"));
  for (const overrides of [{ FAKE_TAG_SHA: "b".repeat(40) }, { GITHUB_SHA: "b".repeat(40), FAKE_TAG_SHA: "b".repeat(40) }]) {
    const failure = f.run("action-smoke", "Verify published release commit", overrides);
    assert.equal(failure.status, 1);
    assert.match(failure.stdout, /Published tag and Action checkout must match/);
  }
});

test("publication fails before invoking GitHub when prepared notes are missing", (t) => {
  const f = fixture(t);
  const failure = f.run("publish", "Create release");
  assert.equal(failure.status, 1);
  assert.deepEqual(f.calls(), []);
});

test("stable promotion rechecks the tag and only updates latest after the major tag", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", { VERSION: "2.1.0", TAG: "v2.1.0" }));
  const calls = f.calls();
  assert.ok(calls.some((args) => args.includes("PATCH") && args.includes(`sha=${f.sha}`)));
  assert.deepEqual(calls.at(-1), ["release", "edit", "v2.1.0", "--repo", "fixture/quality", "--latest"]);
  const before = calls.length;
  const failure = f.run("promote-major-tag", "Update stable major tag and latest release", { FAKE_TAG_SHA: "b".repeat(40) });
  assert.equal(failure.status, 1);
  assert.match(failure.stderr, /refusing release promotion/);
  assert.equal(f.calls().length, before + 1);
});

test("stable zero-major releases can become latest without creating a moving major tag", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    VERSION: "0.9.0", TAG: "v0.9.0", FAKE_LATEST_EXISTS: "false",
  }));
  assert.equal(f.calls().length, 3);
  assert.ok(f.calls().at(-1).includes("--latest"));
});

function mutations(calls) {
  return calls.filter((args) => args.includes("--method") || (args[0] === "release" && args[1] === "edit"));
}

test("rerunning an older release leaves both newer release pointers unchanged", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    FAKE_MAJOR_VERSION: "2.1.0", FAKE_LATEST_TAG: "v2.1.0",
  }));
  assert.deepEqual(mutations(f.calls()), []);
});

test("a lower-major backport advances its own alias without replacing latest", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    VERSION: "2.0.1", TAG: "v2.0.1", FAKE_LATEST_TAG: "v3.0.0",
  }));
  assert.deepEqual(mutations(f.calls()), [[
    "api", "--method", "PATCH", "repos/fixture/quality/git/refs/tags/v2", "-f", `sha=${f.sha}`, "-F", "force=true",
  ]]);
});

test("an already-current release performs no pointer mutations", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    FAKE_MAJOR_SHA: f.sha, FAKE_LATEST_TAG: f.env.TAG,
  }));
  assert.deepEqual(mutations(f.calls()), []);
});

test("the first stable release creates its absent alias and latest pointer", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    FAKE_MAJOR_EXISTS: "false", FAKE_LATEST_EXISTS: "false",
  }));
  assert.equal(mutations(f.calls()).length, 2);
  assert.ok(mutations(f.calls())[0].includes("POST"));
  assert.ok(mutations(f.calls())[1].includes("--latest"));
});

test("equal precedence cannot replace another commit or release build", (t) => {
  const scenarios = [
    {},
    { VERSION: "2.0.0+build.two", TAG: "v2.0.0+build.two", FAKE_MAJOR_VERSION: "2.0.0+build.one" },
    { FAKE_MAJOR_EXISTS: "false", FAKE_LATEST_TAG: "v2.0.0+build.one" },
  ];
  for (const overrides of scenarios) {
    const f = fixture(t);
    const failure = f.run("promote-major-tag", "Update stable major tag and latest release", overrides);
    assert.equal(failure.status, 1);
    assert.match(failure.stderr, /Ambiguous equal-precedence/);
    assert.deepEqual(mutations(f.calls()), []);
  }
});

test("API failures and unresolvable pointer versions fail before any promotion", (t) => {
  const scenarios = [
    { FAKE_API_FAILURE: "/git/ref/tags/v2", FAKE_API_STATUS: "403" },
    { FAKE_API_FAILURE: "/releases/latest", FAKE_API_STATUS: "503" },
    { FAKE_API_FAILURE: "/contents/", FAKE_API_STATUS: "404" },
    { FAKE_MAJOR_VERSION: "not-a-version" },
    { FAKE_MAJOR_VERSION: "1.9.0" },
    { FAKE_LATEST_TAG: "nightly" },
  ];
  for (const overrides of scenarios) {
    const f = fixture(t);
    const failure = f.run("promote-major-tag", "Update stable major tag and latest release", {
      VERSION: "2.1.0", TAG: "v2.1.0", ...overrides,
    });
    assert.equal(failure.status, 1, failure.stdout);
    assert.deepEqual(mutations(f.calls()), []);
  }
});

test("version comparison does not round large SemVer components", (t) => {
  const f = fixture(t);
  assertSuccess(f.run("promote-major-tag", "Update stable major tag and latest release", {
    VERSION: "2.9007199254740993.0", TAG: "v2.9007199254740993.0",
    FAKE_MAJOR_VERSION: "2.9007199254740992.0", FAKE_LATEST_TAG: "v2.9007199254740992.0",
  }));
  assert.equal(mutations(f.calls()).length, 2);
});

test("workflow gates stable promotion on smoke and carries notes without running checkout code with write access", () => {
  const publish = job("publish");
  assert.match(job("prepare"), /node scripts\/prepare-release-notes\.mjs/);
  assert.match(job("prepare"), /name: quality-release-notes/);
  assert.match(publish, /name: quality-release-notes\n          path: release-notes/);
  assert.match(publish, /pattern: release-\*/);
  assert.match(job("action-smoke"), /ref: refs\/tags\/\$\{\{ needs\.prepare\.outputs\.tag \}\}/);
  const promote = job("promote-major-tag");
  assert.match(promote, /needs: \[prepare, action-smoke\]/);
  assert.match(promote, /if: >-\n      always\(\) &&\n      needs\.prepare\.result == 'success' &&\n      needs\.action-smoke\.result == 'success' &&\n      needs\.prepare\.outputs\.prerelease == 'false'/);
  for (const privilegedJob of [publish, promote]) {
    assert.doesNotMatch(privilegedJob, /uses: actions\/checkout|node scripts\/|pnpm |cargo /);
  }
});
