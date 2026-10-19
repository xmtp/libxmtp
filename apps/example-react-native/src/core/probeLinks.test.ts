import assert from "node:assert/strict";
import { test } from "node:test";

import { probeLinkConfirmation } from "./probeLinks.ts";

test("a probe link asks before sign out or local data deletion, and runs other actions", () => {
  const cases: readonly (readonly [string, string | undefined])[] = [
    ["signOut", "signOut"],
    ["delete", "delete"],
    ["connect", undefined],
    ["restore", undefined],
    ["retry", undefined],
    ["continueReset", undefined],
    ["probe", undefined],
    ["handles", undefined],
    ["allowRequests", undefined],
    ["createGroup", undefined],
  ];
  for (const [action, confirmation] of cases) {
    assert.equal(probeLinkConfirmation(action), confirmation, action);
  }
});
