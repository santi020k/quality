import { readFile, writeFile } from "node:fs/promises";

const checkOnly = process.argv.includes("--check");
const actionPackage = JSON.parse(await readFile("packages/action/package.json", "utf8"));
const cliPackage = JSON.parse(await readFile("crates/quality-cli/package.json", "utf8"));
const cargoPath = "crates/quality-cli/Cargo.toml";
const cargoManifest = await readFile(cargoPath, "utf8");
const cargoVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1];
const targetVersion = cliPackage.version;
const majorVersion = Number.parseInt(targetVersion.split(".")[0] ?? "", 10);
const minorVersion = Number.parseInt(targetVersion.split(".")[1] ?? "", 10);
const changedFiles = new Set();

if (!cargoVersion) throw new Error("Could not read the quality-cli Cargo version");
if (actionPackage.version !== cliPackage.version) {
  throw new Error(
    `Release package versions differ: action=${actionPackage.version}, cli=${cliPackage.version}`,
  );
}

if (!Number.isInteger(majorVersion) || !Number.isInteger(minorVersion)) {
  throw new Error(`Invalid release version: ${targetVersion}`);
}

async function synchronize(path, transform) {
  const current = await readFile(path, "utf8");
  const expected = transform(current);

  if (current === expected) return;
  changedFiles.add(path);
  if (!checkOnly) await writeFile(path, expected);
}

await synchronize(cargoPath, (contents) =>
  contents.replace(/^version = "[^"]+"$/m, `version = "${targetVersion}"`),
);

const releaseReferenceFiles = [
  "README.md",
  "install.sh",
  "apps/site/src/content/docs/commands.md",
  "apps/site/src/content/docs/getting-started.md",
  "apps/site/src/content/docs/github-actions.md",
];

for (const path of releaseReferenceFiles) {
  await synchronize(path, (contents) =>
    contents.replace(/v\d+\.\d+\.\d+/g, `v${targetVersion}`),
  );
}

await synchronize("README.md", (contents) => {
  const status =
    majorVersion < 1
      ? `> Status: \`${majorVersion}.${minorVersion}.x\` preview. The configuration format may change before the first\n> stable release.`
      : `> Status: \`${majorVersion}.x\` stable. Public CLI, configuration, report, and Action contracts follow the\n> documented compatibility policy for this major release.`;

  return contents.replace(/^> Status:.*\n> .*release\.$/m, status);
});

await synchronize("apps/site/src/content/docs/compatibility.md", (contents) => {
  const previewPolicy =
    /The latest stable release receives fixes\. During the pre-1\.0 preview, only the\nlatest preview release is supported\./;
  const stablePolicy =
    /The latest stable major release receives fixes\. Older major releases may receive\ncritical security fixes when explicitly announced\./;
  const expectedPolicy =
    majorVersion < 1
      ? "The latest stable release receives fixes. During the pre-1.0 preview, only the\nlatest preview release is supported."
      : "The latest stable major release receives fixes. Older major releases may receive\ncritical security fixes when explicitly announced.";

  return contents.replace(previewPolicy, expectedPolicy).replace(stablePolicy, expectedPolicy);
});

if (!checkOnly && cargoVersion !== targetVersion) {
  await synchronize("CHANGELOG.md", (contents) => {
    if (contents.includes(`## ${targetVersion}`)) return contents;
    return contents.replace(
      "## Unreleased\n\n",
      `## Unreleased\n\n- No changes yet.\n\n## ${targetVersion}\n\n`,
    );
  });
}

if (checkOnly) {
  if (changedFiles.size > 0) {
    throw new Error(
      `Release metadata does not match ${targetVersion}: ${[...changedFiles].join(", ")}`,
    );
  }
  process.stdout.write(`Release metadata agrees at ${targetVersion}.\n`);
} else if (changedFiles.size > 0) {
  process.stdout.write(`Synchronized ${targetVersion}: ${[...changedFiles].join(", ")}\n`);
} else {
  process.stdout.write(`Release metadata already agrees at ${targetVersion}.\n`);
}
