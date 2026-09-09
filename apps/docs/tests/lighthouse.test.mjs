import assert from "node:assert/strict";
import test from "node:test";
import { compareLighthouse } from "../scripts/check-lighthouse.mjs";

const baseline = {
  pages: [{ new: "/", accessibility: 0.95, performance: 0.8 }],
};

test("Lighthouse comparison blocks accessibility regression", () => {
  assert.deepEqual(
    compareLighthouse(
      [{ path: "/", accessibility: 0.94, performance: 1 }],
      baseline,
    ),
    ["/ accessibility 0.94 is below baseline 0.95"],
  );
});

test("performance is blocking only for a cutover run", () => {
  const result = [{ path: "/", accessibility: 1, performance: 0.79 }];
  assert.deepEqual(compareLighthouse(result, baseline), []);
  assert.deepEqual(
    compareLighthouse(result, baseline, { checkPerformance: true }),
    ["/ performance 0.79 is below baseline 0.8"],
  );
});
