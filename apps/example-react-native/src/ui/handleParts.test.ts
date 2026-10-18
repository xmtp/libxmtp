import assert from "node:assert/strict";
import { test } from "node:test";

import { normalizeHandle, splitHandle } from "./handleParts.ts";

test("splits the #code suffix from the name", () => {
  assert.deepEqual(splitHandle("marco#0412"), { name: "marco", code: "#0412" });
});

test("drops exactly one leading @", () => {
  assert.equal(normalizeHandle("@nick"), "nick");
  assert.equal(normalizeHandle("@@nick"), "@nick");
  assert.deepEqual(splitHandle("@lena#2207"), { name: "lena", code: "#2207" });
});

test("splits only at the first #", () => {
  assert.deepEqual(splitHandle("a#b#c"), { name: "a", code: "#b#c" });
});

test("keeps a handle without a code whole", () => {
  assert.deepEqual(splitHandle("nickm"), { name: "nickm", code: "" });
  assert.deepEqual(splitHandle(""), { name: "", code: "" });
});
