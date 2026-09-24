import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const actionPath = "actions/setup-pnpm/action.yml";
const documentationPath = "apps/site/src/content/docs/github-actions.md";
const reusableWorkflowPath = ".github/workflows/reusable-pnpm-ci.yml";

test("the shared pnpm action forwards its registry URL through every Node setup path", async () => {
  const action = await readFile(actionPath, "utf8");
  const registryForwards =
    action.match(/^\s+registry-url: \$\{\{ inputs\.registry-url \}\}$/gm)?.length ?? 0;

  assert.match(action, /^  registry-url:\n    description: .+$/m);
  assert.equal(registryForwards, 4);
});

test("shared pnpm automation offers an opt-in, revision-safe task cache", async () => {
  const [action, reusableWorkflow] = await Promise.all([
    readFile(actionPath, "utf8"),
    readFile(reusableWorkflowPath, "utf8"),
  ]);

  for (const contents of [action, reusableWorkflow]) {
    assert.match(contents, /task-cache-path:/);
    assert.match(contents, /task-cache-key:/);
    assert.match(contents, /task-cache-config-path:/);
    assert.match(contents, /uses: actions\/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9/);
    assert.match(contents, /\$\{\{ github\.sha \}\}/);
    assert.match(contents, /\$\{\{ runner\.arch \}\}/);
    assert.match(contents, /hashFiles\(inputs\.node-version-file,/);
  }

  assert.match(action, /task-cache-hit:/);
});

test("the registry documentation keeps credentials in the consuming workflow", async () => {
  const documentation = await readFile(documentationPath, "utf8");

  assert.match(
    documentation,
    /santi020k\/quality\/actions\/setup-pnpm@eec1701b98bcc0b76d36af288ee78a0369cd84cc/,
  );
  assert.match(documentation, /registry-url: https:\/\/registry\.npmjs\.org/);
  assert.match(documentation, /NODE_AUTH_TOKEN: \$\{\{ secrets\.NPM_TOKEN \}\}/);
  assert.match(documentation, /dependency installation also requires a\nprivate registry/);
  assert.match(documentation, /task-cache-path: \.turbo\/cache/);
  assert.match(documentation, /task-cache-key: quality/);
});
