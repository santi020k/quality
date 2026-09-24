import assert from "node:assert/strict";
import test from "node:test";

import {
  ensureReleaseHeading,
  hasReleaseHeading,
  releaseNotesForVersion,
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

test("preserves version annotations for immutable Action commit pins", () => {
  const pinnedAction =
    "uses: example/action@0123456789abcdef0123456789abcdef01234567 # v1.1.1";

  assert.equal(replaceReleaseVersionReferences(pinnedAction, "1.1.2"), pinnedAction);
});

test("matches only an exact release heading", () => {
  const prereleaseChangelog = "## Unreleased\n\n## 1.0.0-next.0\n";

  assert.equal(hasReleaseHeading(prereleaseChangelog, "1.0.0"), false);
  assert.equal(hasReleaseHeading(prereleaseChangelog, "1.0.0-next.0"), true);
});

test("inserts a missing release heading and remains idempotent", () => {
  const prereleaseChangelog = "## Unreleased\n\n- No changes yet.\n\n## 1.0.0-next.0\n";
  const expected =
    "## Unreleased\n\n- No changes yet.\n\n## 1.0.0\n\n- Stable release.\n\n## 1.0.0-next.0\n";

  assert.equal(
    ensureReleaseHeading(prereleaseChangelog, "1.0.0", "- Stable release."),
    expected,
  );
  assert.equal(ensureReleaseHeading(expected, "1.0.0", "- Stable release."), expected);
});

test("extracts release notes from a package changelog", () => {
  const changelog = "# Package\n\n## 1.1.1\n\n### Patch Changes\n\n- Added registry support.\n\n## 1.1.0\n";

  assert.equal(
    releaseNotesForVersion(changelog, "1.1.1"),
    "### Patch Changes\n\n- Added registry support.",
  );
  assert.equal(releaseNotesForVersion(changelog, "2.0.0"), "");
});
