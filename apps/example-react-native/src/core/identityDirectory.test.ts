import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type IdentityLookup,
  type InboxIdentity,
  IdentityDirectory,
} from "./identityDirectory.ts";

test("a handle change keeps the other handles (handleChangePreservesOtherHandles)", () => {
  const directory = new IdentityDirectory(
    () => Promise.resolve([]),
    () => true,
  );
  directory.merge([
    {
      inboxId: "inbox",
      handle: "first",
      otherHandles: ["older", "first", "next"],
    },
  ]);
  directory.handleChanged("inbox", "next");
  assert.deepEqual(directory.identity("inbox")?.otherHandles, [
    "first",
    "older",
  ]);
  assert.equal(directory.displayName("inbox"), "@next");
  directory.handleChanged("inbox", "latest", [
    "authoritative",
    "authoritative",
    "latest",
  ]);
  assert.deepEqual(directory.identity("inbox")?.otherHandles, [
    "authoritative",
  ]);
});

test("an event wins over an inbox read that started earlier (handleEventWinsOverEarlierInboxRead)", async () => {
  let respond: ((entries: readonly InboxIdentity[]) => void) | undefined;
  const lookup: IdentityLookup = () =>
    new Promise((resolve) => {
      respond = resolve;
    });
  const directory = new IdentityDirectory(lookup, () => true);
  const loading = directory.load(["inbox", "other"]);
  directory.handleChanged("inbox", "fresh");
  respond?.([
    { inboxId: "inbox", handle: "stale", otherHandles: [] },
    { inboxId: "other", handle: "peer", otherHandles: [] },
  ]);
  await loading;
  assert.equal(directory.identity("inbox")?.handle, "fresh");
  assert.equal(directory.identity("other")?.handle, "peer");
});

test("display names fall back to short inbox IDs; an ended session merges nothing", () => {
  let valid = true;
  const directory = new IdentityDirectory(
    () => Promise.resolve([]),
    () => valid,
  );
  const inbox = "0123456789abcdef0123";
  assert.equal(directory.displayName(inbox), "0123456789ab");
  assert.equal(directory.displayName(inbox, true), "01234567");
  let changes = 0;
  directory.subscribe(() => {
    changes += 1;
  });
  valid = false;
  directory.merge([{ inboxId: inbox, handle: "late", otherHandles: [] }]);
  assert.equal(directory.identity(inbox), undefined);
  assert.equal(changes, 0);
});
