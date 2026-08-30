const prerelease = "(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const buildMetadata = "(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const releaseVersionReference = new RegExp(
  `v\\d+\\.\\d+\\.\\d+${prerelease}${buildMetadata}`,
  "g",
);

export function replaceReleaseVersionReferences(contents, targetVersion) {
  return contents.replace(releaseVersionReference, `v${targetVersion}`);
}

export function hasReleaseHeading(contents, targetVersion) {
  return contents.split("\n").some((line) => line === `## ${targetVersion}`);
}

export function ensureReleaseHeading(contents, targetVersion) {
  if (hasReleaseHeading(contents, targetVersion)) return contents;

  return contents.replace(
    "## Unreleased\n\n",
    `## Unreleased\n\n- No changes yet.\n\n## ${targetVersion}\n\n`,
  );
}
