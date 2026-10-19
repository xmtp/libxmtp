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

test("only the latest load applies, and a failure shows the load error", async () => {
  const slow = deferred();
  let calls = 0;
  let fail = false;
  const store = new SharedMediaStore(
    CONVERSATION,
    {
      conversation: (id) =>
        Promise.resolve({ id, kind: "dm", consent: "allowed" }),
      media: async () => {
        calls += 1;
        if (calls === 1) {
          await slow.promise;
          return [fileMessage(1, "old.pdf")];
        }
        if (fail) throw new Error("offline");
        return [fileMessage(2, "new.pdf")];
      },
    },
    () => true,
  );
  const first = store.load();
  await store.load();
  slow.resolve();
  await first;
  assert.deepEqual(
    store.getState().rows.map((row) => row.id),
    ["m0002"],
  );
  fail = true;
  await store.load();
  assert.equal(store.getState().error, SharedMediaCopy.loadFailed);
});
