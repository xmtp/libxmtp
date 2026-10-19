import assert from "node:assert/strict";
import { test } from "node:test";

import { SessionFence } from "./sessionFence.ts";

test("a new reservation rejects the old session of the same profile", () => {
  const fence = new SessionFence();
  const first = fence.bind("a", fence.reserve());
  assert.ok(first !== undefined);
  assert.ok(fence.accepts(first));
  // Sign out and join the same server again: same profile, new generation.
  const second = fence.bind("a", fence.reserve());
  assert.ok(second !== undefined);
  assert.ok(!fence.accepts(first));
  assert.ok(fence.accepts(second));
});

test("a restore cannot override an explicit action", () => {
  const fence = new SessionFence();
  const restore = fence.reserveRestore();
  assert.equal(restore, 1);
  fence.reserve();
  assert.equal(fence.reserveRestore(), undefined);
  // The explicit reservation stays sticky for the process.
  assert.equal(fence.reserveRestore(), undefined);
});

test("a restore cannot reserve over a bound profile", () => {
  const fence = new SessionFence();
  const generation = fence.reserveRestore();
  assert.ok(generation !== undefined);
  const key = fence.bind("a", generation);
  assert.ok(key !== undefined);
  assert.equal(fence.reserveRestore(), undefined);
  assert.ok(fence.accepts(key));
});

test("binding needs the current, unbound reservation", () => {
  const fence = new SessionFence();
  const old = fence.reserve();
  const current = fence.reserve();
  assert.equal(fence.bind("a", old), undefined);
  assert.ok(fence.bind("a", current) !== undefined);
  assert.equal(fence.bind("b", current), undefined);
  assert.ok(!fence.accepts({ profileId: "b", generation: current }));
  assert.equal(fence.currentGeneration(), current);
});
