const prerelease = "(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const buildMetadata = "(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const releaseVersionReference = new RegExp(
  `v\\d+\\.\\d+\\.\\d+${prerelease}${buildMetadata}`,
  "g",
);

export function replaceReleaseVersionReferences(contents, targetVersion) {
  return contents.replace(releaseVersionReference, `v${targetVersion}`);
}
