import { execFileSync } from "node:child_process";
import { appendFile, readdir } from "node:fs/promises";
import { pathToFileURL } from "node:url";

export async function checkConsumedChangesets(root = ".") {
  const files = await readdir(`${root}/.changeset`);
  const pending = files.filter((file) => file.endsWith(".md") && file !== "README.md");
  if (pending.length > 0) {
    throw new Error(`Release branches must consume all Changesets before merging: ${pending.sort().join(", ")}. Run pnpm version-packages on the selected release branch.`);
  }
}

export function activeReleaseBranches(root = ".") {
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const refs = git("for-each-ref", "--format=%(refname)", "refs/remotes/origin/release/v*").split("\n").filter(Boolean);
  return refs.filter((ref) => {
    const tag = ref.slice("refs/remotes/origin/release/".length);
    try {
      git("merge-base", "--is-ancestor", ref, "refs/remotes/origin/main");
      git("show-ref", "--verify", `refs/tags/${tag}`);
      return false;
    } catch {
      return true;
    }
  }).map((ref) => ref.slice("refs/remotes/origin/".length));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv[2] === "consumed") {
      await checkConsumedChangesets();
    } else if (process.argv[2] === "active") {
      const active = activeReleaseBranches();
      if (!process.env.GITHUB_OUTPUT) throw new Error("Missing GITHUB_OUTPUT.");
      await appendFile(process.env.GITHUB_OUTPUT, `active=${active.length > 0}\n`);
      process.stdout.write(active.length > 0
        ? `Keep version preparation on the active release branch: ${active.join(", ")}.\n`
        : "No active deliberate release branch; automated version preparation may proceed.\n");
    } else {
      throw new Error("Usage: node scripts/check-release-preparation.mjs <consumed|active>");
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : "Release preparation check failed."}\n`);
    process.exitCode = 1;
  }
}
