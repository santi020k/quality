import { readFile, writeFile } from "node:fs/promises";

const checkOnly = process.argv.includes("--check");
const actionPackage = JSON.parse(await readFile("packages/action/package.json", "utf8"));
const cliPackage = JSON.parse(await readFile("crates/quality-cli/package.json", "utf8"));
const cargoPath = "crates/quality-cli/Cargo.toml";
const cargoManifest = await readFile(cargoPath, "utf8");
const cargoVersion = cargoManifest.match(/^version = "([^"]+)"$/m)?.[1];

if (!cargoVersion) throw new Error("Could not read the quality-cli Cargo version");
if (actionPackage.version !== cliPackage.version) {
  throw new Error(
    `Release package versions differ: action=${actionPackage.version}, cli=${cliPackage.version}`,
  );
}

if (checkOnly) {
  if (cargoVersion !== cliPackage.version) {
    throw new Error(`Cargo version ${cargoVersion} does not match ${cliPackage.version}`);
  }
  process.stdout.write(`Release versions agree at ${cargoVersion}.\n`);
} else if (cargoVersion !== cliPackage.version) {
  await writeFile(
    cargoPath,
    cargoManifest.replace(
      /^version = "[^"]+"$/m,
      `version = "${cliPackage.version}"`,
    ),
  );
  process.stdout.write(`Updated Cargo version to ${cliPackage.version}.\n`);
}
