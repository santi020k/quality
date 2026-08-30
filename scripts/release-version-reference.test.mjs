import assert from "node:assert/strict";
import test from "node:test";

import { replaceReleaseVersionReferences } from "./release-version-reference.mjs";

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
