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

export function mergeReleaseNotes(notes) {
  const sections = new Map();

  for (const note of notes) {
    for (const section of note.split(/^### /m).slice(1)) {
      const headingEnd = section.indexOf("\n");
      if (headingEnd === -1) continue;

      const heading = section.slice(0, headingEnd).trim();
      const entries = section
        .slice(headingEnd + 1)
        .trim()
        .split(/\n\n(?=- )/)
        .filter(Boolean);
      const mergedEntries = sections.get(heading) ?? [];

      for (const entry of entries) {
        if (!mergedEntries.includes(entry)) mergedEntries.push(entry);
      }
      sections.set(heading, mergedEntries);
    }
  }

  return [...sections]
    .map(([heading, entries]) => `### ${heading}\n\n${entries.join("\n\n")}`)
    .join("\n\n");
}

export function ensureReleaseHeading(contents, targetVersion, releaseNotes = "- No changes yet.") {
  if (hasReleaseHeading(contents, targetVersion)) return contents;

  const unreleased = /## Unreleased\n\n(?<notes>[\s\S]*?)(?=\n## )/;
  return contents.replace(unreleased, (section) =>
    `${section.trimEnd()}\n\n## ${targetVersion}\n\n${releaseNotes.trim()}\n`,
  );
}
