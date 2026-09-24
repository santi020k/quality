const prerelease = "(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const buildMetadata = "(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const releaseVersionReference = new RegExp(
  `v\\d+\\.\\d+\\.\\d+${prerelease}${buildMetadata}`,
  "g",
);

export function replaceReleaseVersionReferences(contents, targetVersion) {
  return contents.replace(releaseVersionReference, (reference, offset) => {
    const annotationPrefix = contents.slice(Math.max(0, offset - 44), offset);
    return /@[0-9a-f]{40} # $/.test(annotationPrefix) ? reference : `v${targetVersion}`;
  });
}

export function hasReleaseHeading(contents, targetVersion) {
  return contents.split("\n").some((line) => line === `## ${targetVersion}`);
}

export function releaseNotesForVersion(contents, targetVersion) {
  const heading = `## ${targetVersion}\n`;
  const start = contents.indexOf(heading);
  if (start === -1) return "";
  const bodyStart = start + heading.length;
  const nextHeading = contents.indexOf("\n## ", bodyStart);
  return contents.slice(bodyStart, nextHeading === -1 ? undefined : nextHeading).trim();
}

export function ensureReleaseHeading(contents, targetVersion, releaseNotes = "- No changes yet.") {
  if (hasReleaseHeading(contents, targetVersion)) return contents;

  const unreleased = /## Unreleased\n\n(?<notes>[\s\S]*?)(?=\n## )/;
  return contents.replace(unreleased, (section) =>
    `${section.trimEnd()}\n\n## ${targetVersion}\n\n${releaseNotes.trim()}\n`,
  );
}
