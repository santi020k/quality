import { appendFile, readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const commitPattern = /^[0-9a-f]{40}$/u;

export async function deploymentEligibility({ repository, sha, version }, api) {
  if (!/^[\w.-]+\/[\w.-]+$/u.test(repository) || !commitPattern.test(sha)) {
    throw new Error("Deployment requires a repository and an exact source commit.");
  }
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?$/u.test(version)) {
    return { eligible: false, reason: "Production documentation requires a stable release version." };
  }
  const prefix = `/repos/${repository}`;
  const current = await api(`${prefix}/commits/${encodeURIComponent("refs/heads/main")}`);
  if (current?.sha !== sha) {
    return { eligible: false, reason: "A newer main revision superseded this deployment." };
  }
  const tag = `v${version}`;
  const release = await api(`${prefix}/releases/tags/${encodeURIComponent(tag)}`);
  if (!release || release.draft !== false || release.prerelease !== false || release.tag_name !== tag) {
    return { eligible: false, reason: `Waiting for the published stable release ${tag}.` };
  }
  const tagged = await api(`${prefix}/commits/${encodeURIComponent(`refs/tags/${tag}`)}`);
  if (!commitPattern.test(tagged?.sha ?? "")) throw new Error(`Cannot resolve release tag ${tag}.`);
  const comparison = await api(`${prefix}/compare/${tagged.sha}...${sha}`);
  if (comparison?.status !== "ahead" && comparison?.status !== "identical") {
    throw new Error(`Release tag ${tag} is not an ancestor of the documentation source.`);
  }
  const runs = await api(`${prefix}/actions/workflows/release.yml/runs?head_sha=${tagged.sha}&status=success&per_page=100`);
  if (!Array.isArray(runs?.workflow_runs)) throw new Error("Cannot inspect release verification runs.");
  const candidates = runs.workflow_runs.filter((run) =>
    run?.head_sha === tagged.sha && run.head_branch === "main" &&
    run.head_repository?.full_name === repository && run.conclusion === "success" &&
    (run.event === "push" || run.event === "workflow_dispatch"));
  let verified = false;
  for (const run of candidates) {
    if (!Number.isSafeInteger(run.id) || run.id < 1) throw new Error("Release verification run has no valid identity.");
    const result = await api(`${prefix}/actions/runs/${run.id}/jobs?filter=latest&per_page=100`);
    if (!Array.isArray(result?.jobs)) throw new Error("Cannot inspect release verification jobs.");
    const requiredJobs = ["Verify published Action", "Promote verified stable release"];
    if (requiredJobs.every((name) => result.jobs.some((job) => job?.name === name && job.conclusion === "success"))) {
      verified = true;
      break;
    }
  }
  if (!verified) {
    return { eligible: false, reason: `Waiting for successful Release verification of ${tag}.` };
  }
  return { eligible: true, sha, tag, reason: `Deploying documentation for verified release ${tag}.` };
}

export async function githubApi(path, { token, baseUrl = "https://api.github.com", request = fetch }) {
  const response = await request(`${baseUrl}${path}`, {
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "X-GitHub-Api-Version": "2026-03-10",
    },
  });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`GitHub deployment verification failed (HTTP ${response.status}).`);
  return response.json();
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const manifest = JSON.parse(await readFile("packages/action/package.json", "utf8"));
    const cli = JSON.parse(await readFile("crates/quality-cli/package.json", "utf8"));
    if (manifest.version !== cli.version) throw new Error("Action and CLI versions disagree.");
    if (!process.env.GH_TOKEN || !process.env.GITHUB_OUTPUT) throw new Error("Missing deployment workflow context.");
    const result = await deploymentEligibility({
      repository: process.env.GITHUB_REPOSITORY ?? "",
      sha: process.env.GITHUB_SHA ?? "",
      version: manifest.version,
    }, (path) => githubApi(path, { token: process.env.GH_TOKEN }));
    await appendFile(process.env.GITHUB_OUTPUT, `eligible=${result.eligible}\nsha=${result.sha ?? ""}\n`);
    process.stdout.write(`${result.reason}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : "Deployment verification failed."}\n`);
    process.exitCode = 1;
  }
}
