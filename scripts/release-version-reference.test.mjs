import assert from "node:assert/strict";
import test from "node:test";

import {
  ensureReleaseHeading,
  hasReleaseHeading,
  replaceReleaseVersionReferences,
} from "./release-version-reference.mjs";

test("replaces stable, prerelease, and build-metadata version references", () => {
  const contents = [
    "stable v1.0.0",
    "prerelease v1.1.0-next.0",
    "build v1.1.0-next.0+sha.123",
  ].join("\n");

  assert.equal(
    replaceReleaseVersionReferences(contents, "1.1.0-next.1"),
    [
      "stable v1.1.0-next.1",
      "prerelease v1.1.0-next.1",
      "build v1.1.0-next.1",
    ].join("\n"),
  );
});

test("matches only an exact release heading", () => {
  const prereleaseChangelog = "## Unreleased\n\n## 1.0.0-next.0\n";

  assert.equal(hasReleaseHeading(prereleaseChangelog, "1.0.0"), false);
  assert.equal(hasReleaseHeading(prereleaseChangelog, "1.0.0-next.0"), true);
});

test("inserts a missing release heading and remains idempotent", () => {
  const prereleaseChangelog = "## Unreleased\n\n## 1.0.0-next.0\n";
  const expected =
    "## Unreleased\n\n- No changes yet.\n\n## 1.0.0\n\n## 1.0.0-next.0\n";

  assert.equal(ensureReleaseHeading(prereleaseChangelog, "1.0.0"), expected);
  assert.equal(ensureReleaseHeading(expected, "1.0.0"), expected);
});
