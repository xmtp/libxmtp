import assert from "node:assert/strict";
import { test } from "node:test";

import {
  RecipientCopy,
  RecipientError,
  parseRecipient,
  recipientEntries,
  recipientLabel,
  recipientNotFound,
} from "./recipients.ts";

const problem = (input: string): string => {
  try {
    parseRecipient(input);
  } catch (error) {
    assert.ok(error instanceof RecipientError);
    return error.problem;
  }
  assert.fail(`${input} parsed`);
};

test("parses handles, addresses and inbox IDs", () => {
  assert.equal(problem(" "), "empty");
  assert.equal(problem("0xabc"), "invalidAddress");
  assert.equal(problem("z".repeat(64)), "invalidInbox");
  assert.deepEqual(parseRecipient(" @A#1234 "), {
    kind: "handle",
    handle: "a#1234",
  });
  assert.deepEqual(parseRecipient("0X" + "A".repeat(40)), {
    kind: "ethereum",
    address: "0x" + "a".repeat(40),
  });
  assert.deepEqual(parseRecipient("B".repeat(64)), {
    kind: "inbox",
    inboxId: "b".repeat(64),
  });
  assert.deepEqual(parseRecipient("a"), { kind: "handle", handle: "a" });
});

test("invalid handles and addresses", () => {
  assert.equal(problem("0x" + "g".repeat(40)), "invalidAddress");
  assert.equal(problem("0x" + "a".repeat(41)), "invalidAddress");
  assert.equal(problem("@"), "invalidHandle");
  assert.equal(problem("@@nick"), "invalidHandle");
  assert.equal(problem("nick#12"), "invalidHandle");
  assert.equal(problem("1nick"), "invalidHandle");
  assert.equal(problem("a".repeat(63)), "invalidHandle");
  assert.equal(
    new RecipientError("invalidHandle").message,
    RecipientCopy.invalidHandle,
  );
  assert.equal(
    RecipientCopy.invalidHandle,
    "Enter a handle such as nick or nick#2341, an Ethereum address, or an inbox ID.",
  );
});

test("not-found copy depends on the kind", () => {
  assert.equal(
    recipientNotFound(parseRecipient("a")).message,
    "No XMTP account has this handle.",
  );
  assert.equal(
    recipientNotFound(parseRecipient("0x" + "a".repeat(40))).message,
    "This address has no XMTP account.",
  );
});

test("comma lists drop empty entries and labels shorten inbox IDs", () => {
  assert.deepEqual(recipientEntries("@a,,b, c,"), ["@a", "b", " c"]);
  assert.deepEqual(recipientEntries(""), []);
  assert.equal(recipientLabel("abcdef0123456789", "nick"), "@nick");
  assert.equal(recipientLabel("abcdef0123456789"), "abcdef012345");
});
