import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type IdentityLookup,
  type InboxIdentity,
  IdentityDirectory,
} from "./identityDirectory.ts";

test("a handle change keeps the other handles", () => {
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
  assert.equal(directory.identity("inbox")?.handle, "next");
  assert.deepEqual(directory.identity("inbox")?.otherHandles, [
    "first",
    "older",
  ]);
});

test("an event wins over an inbox read that started earlier; a read that ends after the session merges nothing", async () => {
  let respond: ((entries: readonly InboxIdentity[]) => void) | undefined;
  const lookup: IdentityLookup = () =>
    new Promise((resolve) => {
      respond = resolve;
    });
  let valid = true;
  const directory = new IdentityDirectory(lookup, () => valid);
  const loading = directory.load(["inbox", "other"]);
  directory.handleChanged("inbox", "fresh");
  respond?.([
    { inboxId: "inbox", handle: "stale", otherHandles: [] },
    { inboxId: "other", handle: "peer", otherHandles: [] },
  ]);
  await loading;
  assert.equal(directory.identity("inbox")?.handle, "fresh");
  assert.equal(directory.identity("other")?.handle, "peer");
  let changes = 0;
  directory.subscribe(() => {
    changes += 1;
  });
  const late = directory.load(["late"]);
  valid = false;
  respond?.([{ inboxId: "late", handle: "late", otherHandles: [] }]);
  await late;
  assert.equal(directory.identity("late"), undefined);
  assert.equal(changes, 0);
});
