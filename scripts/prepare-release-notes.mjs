import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { releaseNotesForVersion } from "./release-version-reference.mjs";

export function prepareReleaseNotes(changelog, version) {
  const notes = releaseNotesForVersion(changelog, version);
  if (!notes) {
    throw new Error(`CHANGELOG.md has no prepared release notes for ${version}.`);
  }
  return `${notes}\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const [version, outputPath, ...extra] = process.argv.slice(2);
    if (!version || !outputPath || extra.length > 0) {
      throw new Error("Usage: node scripts/prepare-release-notes.mjs <version> <output-path>");
    }
    const notes = prepareReleaseNotes(readFileSync("CHANGELOG.md", "utf8"), version);
    writeFileSync(outputPath, notes);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
