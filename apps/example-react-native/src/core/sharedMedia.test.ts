import assert from "node:assert/strict";
import { test } from "node:test";

import { remoteFile } from "./attachmentFakes.ts";
import { CONVERSATION, textMessage } from "./chatFakes.ts";
import type { ChatMessage, ConversationRecord } from "./chatMessages.ts";
import {
  SharedMediaCopy,
  SharedMediaStore,
  mediaSections,
} from "./sharedMedia.ts";
import { deferred } from "./testing.ts";

function fileMessage(n: number, name: string): ChatMessage {
  return {
    ...textMessage(n),
    content: { kind: "attachment", file: remoteFile(name) },
  };
}

test("photos and videos are split from other files by name, keeping order", () => {
  const rows = [
    fileMessage(3, "clip.mov"),
    fileMessage(2, "plan.pdf"),
    fileMessage(1, "Big Sur coastline.jpg"),
    textMessage(0),
  ];
  const { visual, files } = mediaSections(rows);
  assert.deepEqual(
    visual.map((row) => row.id),
    ["m0003", "m0001"],
  );
  assert.deepEqual(
    files.map((row) => row.id),
    ["m0002"],
  );
});

test("a reload closes downloads until the new consent read finishes", async () => {
  let consent: ConversationRecord["consent"] = "allowed";
  const gate = deferred();
  let hold = false;
  const store = new SharedMediaStore(
    CONVERSATION,
    {
      conversation: async (id) => {
        if (hold) await gate.promise;
        return { id, kind: "dm", consent };
      },
      media: () => Promise.resolve([fileMessage(1, "a.jpg")]),
    },
    () => true,
  );
  await store.load();
  assert.equal(store.getState().allowed, true);
  consent = "denied";
  hold = true;
  const reload = store.load();
  assert.equal(store.getState().allowed, false);
  gate.resolve();
  await reload;
  assert.equal(store.getState().allowed, false);
  assert.equal(store.getState().rows.length, 1);
});

test("only the latest load applies, a failure shows the load error, and Retry clears it", async () => {
  let rows: readonly ChatMessage[] = [];
  let hold: Promise<void> | undefined;
  let fail = false;
  const store = new SharedMediaStore(
    CONVERSATION,
    {
      conversation: async (id) => {
        const held = hold;
        hold = undefined;
        if (held !== undefined) await held;
        return { id, kind: "dm", consent: "allowed" };
      },
      media: () =>
        fail ? Promise.reject(new Error("offline")) : Promise.resolve(rows),
    },
    () => true,
  );
  const slow = deferred();
  hold = slow.promise;
  const first = store.load();
  rows = [fileMessage(2, "new.pdf")];
  await store.load();
  // The first load ends last, with the rows from before the second.
  rows = [fileMessage(1, "old.pdf")];
  slow.resolve();
  await first;
  assert.deepEqual(
    store.getState().rows.map((row) => row.id),
    ["m0002"],
  );
  // A superseded load that fails shows no error; the latest failure does.
  const late = deferred();
  hold = late.promise;
  const stale = store.load();
  await store.load();
  fail = true;
  late.resolve();
  await stale;
  assert.equal(store.getState().error, undefined);
  await store.load();
  assert.equal(store.getState().error, SharedMediaCopy.loadFailed);
  // Retry: a load that works clears the error, shows its rows and opens
  // downloads again.
  fail = false;
  rows = [fileMessage(3, "retry.pdf")];
  await store.load();
  assert.equal(store.getState().error, undefined);
  assert.equal(store.getState().allowed, true);
  assert.deepEqual(
    store.getState().rows.map((row) => row.id),
    ["m0003"],
  );
});
