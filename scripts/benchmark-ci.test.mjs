import assert from "node:assert/strict";
import test from "node:test";
import { deterministicSteps } from "./benchmark-ci.mjs";

test("benchmark expands the actual full gate and excludes only the separately measured live audit", () => {
  const steps = deterministicSteps({ mode: "full" }, "pnpm run check && pnpm run test && pnpm run check:security && pnpm run build");
  assert.deepEqual(steps.map((step) => step.args), [["run", "check"], ["run", "test"], ["run", "build"]]);
});

test("benchmark preserves selected tasks and rejects unsupported full-gate commands", () => {
  const steps = [{ id: "check", args: ["run", "check"] }, { id: "security", args: ["run", "check:security"] }];
  assert.deepEqual(deterministicSteps({ mode: "affected", steps }, ""), [steps[0]]);
  assert.throws(() => deterministicSteps({ mode: "full" }, "custom validation"), /Unsupported validation command/);
});
