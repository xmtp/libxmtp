import assert from "node:assert/strict";
import { test } from "node:test";

import { RecipientError, parseRecipient } from "./recipients.ts";

const problem = (input: string): string => {
  try {
    parseRecipient(input);
  } catch (error) {
    assert.ok(error instanceof RecipientError);
    return error.problem;
  }
  assert.fail(`${input} parsed`);
};

test("parses handles, addresses and inbox IDs, and names each problem", () => {
  assert.deepEqual(parseRecipient(" @A#1234 "), {
    kind: "handle",
    handle: "a#1234",
  });
  // Lookup accepts handles shorter than the claim minimum.
  assert.deepEqual(parseRecipient("a"), { kind: "handle", handle: "a" });
  assert.deepEqual(parseRecipient("0X" + "A".repeat(40)), {
    kind: "ethereum",
    address: "0x" + "a".repeat(40),
  });
  assert.deepEqual(parseRecipient("B".repeat(64)), {
    kind: "inbox",
    inboxId: "b".repeat(64),
  });
  for (const [input, expected] of [
    [" ", "empty"],
    ["0xabc", "invalidAddress"],
    ["0x" + "g".repeat(40), "invalidAddress"],
    ["0x" + "a".repeat(41), "invalidAddress"],
    ["z".repeat(64), "invalidInbox"],
    ["@", "invalidHandle"],
    ["@@nick", "invalidHandle"],
    ["nick#12", "invalidHandle"],
    ["1nick", "invalidHandle"],
    ["a".repeat(63), "invalidHandle"],
  ] as const) {
    assert.equal(problem(input), expected, input);
  }
});
