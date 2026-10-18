import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CONVERSATION,
  FakeConversation,
  OWN,
  settle,
  textMessage,
} from "./chatFakes.ts";
import { type ReadingAnchor, TranscriptCache } from "./chatHistory.ts";
import type { ConversationRecord } from "./chatMessages.ts";
import { ChatTimeline } from "./chatTimeline.ts";

interface Harness {
  readonly queries: FakeConversation;
  readonly timeline: ChatTimeline;
  readonly saved: ReadingAnchor[];
  readonly marks: { conversation: ConversationRecord; marker: bigint }[];
  open: boolean;
}

function harness(
  queries: FakeConversation,
  options: { anchor?: ReadingAnchor; cache?: TranscriptCache } = {},
): Harness {
  const saved: ReadingAnchor[] = [];
  const marks: { conversation: ConversationRecord; marker: bigint }[] = [];
  const result: Harness = {
    queries,
    saved,
    marks,
    open: true,
    timeline: undefined as unknown as ChatTimeline,
  };
  const timeline = new ChatTimeline({
    conversationId: CONVERSATION,
    queries,
    cache: options.cache ?? new TranscriptCache(),
    admit: () => result.open,
    readAnchor: () => options.anchor,
    saveAnchor: (anchor) => {
      saved.push(anchor);
    },
    markRead: (conversation, marker) => {
      marks.push({ conversation, marker });
    },
  });
  return Object.assign(result, { timeline });
}

const visibleIds = (h: Harness, from: number, to: number) =>
  h.timeline
    .getState()
    .messages.slice(from, to)
    .map((row) => row.id);

const anchorOf = (n: number): ReadingAnchor => {
  const row = textMessage(n);
  assert.ok(row.position !== undefined);
  return { messageId: row.id, position: row.position };
};

const foreground = { foreground: true, current: true };

test("open loads the newest page, oldest first, with pending rows last", async () => {
  const queries = new FakeConversation(135);
  queries.unpublished.push({
    ...textMessage(500, { sender: OWN }),
    delivery: "failed",
  });
  const h = harness(queries);
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.loaded, true);
  assert.equal(state.messages.length, 51);
  assert.equal(state.messages[0]?.id, "m0086");
  assert.equal(state.messages.at(-2)?.id, "m0135");
  assert.equal(state.messages.at(-1)?.delivery, "failed");
  assert.equal(state.window.hasOlder, true);
  assert.equal(state.atNewest, true);
  assert.equal(state.target, undefined);
});

test("older pages load at the top without duplicates", async () => {
  const h = harness(new FakeConversation(135));
  await h.timeline.load();
  await h.timeline.older();
  await h.timeline.older();
  const { messages, window } = h.timeline.getState();
  assert.equal(messages.length, 135);
  assert.equal(new Set(messages.map((row) => row.id)).size, 135);
  assert.equal(messages[0]?.id, "m0001");
  assert.equal(window.hasOlder, false);
  assert.equal(window.atNewest, true);
});

test("a stream refresh keeps loaded history and adds the new message", async () => {
  const queries = new FakeConversation(135);
  const h = harness(queries);
  await h.timeline.load();
  await h.timeline.older();
  queries.published.push(textMessage(136));
  await h.timeline.refresh();
  const { messages } = h.timeline.getState();
  assert.equal(messages.length, 101);
  assert.equal(messages.at(-1)?.id, "m0136");
  assert.equal(messages[0]?.id, "m0036");
});

test("reopening restores the saved anchor in reading mode", async () => {
  const h = harness(new FakeConversation(135), { anchor: anchorOf(40) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, "m0040");
  assert.equal(state.targetRevision, 1);
  assert.equal(state.atNewest, false);
  assert.equal(state.window.notice, undefined);
  assert.ok(state.messages.some((row) => row.id === "m0040"));
});

test("a deleted anchor restores to the nearest survivor with Position changed", async () => {
  const queries = new FakeConversation(135);
  queries.redact("m0040");
  const h = harness(queries, { anchor: anchorOf(40) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, "m0041");
  assert.equal(state.window.notice, "Position changed");
  assert.equal(state.atNewest, false);
});

test("an anchor without survivors near it falls back to the newest page", async () => {
  const queries = new FakeConversation(3);
  for (const id of ["m0001", "m0002", "m0003"]) queries.redact(id);
  const h = harness(queries, { anchor: anchorOf(2) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, undefined);
  assert.equal(state.window.notice, "Position changed");
  assert.equal(state.messages.at(-1)?.id, "m0003");
});

test("visibility saves the topmost surviving row as the anchor", async () => {
  const h = harness(new FakeConversation(10));
  await h.timeline.load();
  h.timeline.visible(visibleIds(h, 2, 6), foreground);
  assert.deepEqual(h.saved.at(-1), anchorOf(3));
  assert.equal(h.timeline.getState().anchorId, "m0003");
  assert.equal(h.timeline.getState().atNewest, false);
  // The same anchor is not written twice.
  h.timeline.visible(visibleIds(h, 2, 7), foreground);
  assert.equal(h.saved.length, 1);
});

test("the read marker advances only in the foreground, current, at the newest row", async () => {
  const h = harness(new FakeConversation(10));
  await h.timeline.load();
  h.timeline.visible(visibleIds(h, 5, 10), {
    foreground: false,
    current: true,
  });
  h.timeline.visible(visibleIds(h, 5, 10), {
    foreground: true,
    current: false,
  });
  h.timeline.visible(visibleIds(h, 2, 8), foreground);
  assert.equal(h.marks.length, 0);
  h.timeline.visible(visibleIds(h, 5, 10), foreground);
  assert.equal(h.marks.length, 1);
  assert.equal(h.marks[0]?.marker, textMessage(10).insertedAtNs);
  assert.equal(h.timeline.getState().atNewest, true);
});

test("an older page that is not the newest never advances the marker", async () => {
  const h = harness(new FakeConversation(135), { anchor: anchorOf(20) });
  await h.timeline.load();
  assert.equal(h.timeline.getState().window.atNewest, false);
  const ids = h.timeline.getState().messages.map((row) => row.id);
  h.timeline.visible(["m0020"], foreground);
  h.timeline.visible(ids.slice(-3), foreground);
  assert.equal(h.marks.length, 0);
});

test("while restoring, visibility waits for the target row", async () => {
  const h = harness(new FakeConversation(135), { anchor: anchorOf(40) });
  await h.timeline.load();
  // The list first reports its bottom rows; the reading position stays.
  h.timeline.visible(["m0089", "m0090"], foreground);
  assert.equal(h.timeline.getState().target, "m0040");
  assert.equal(h.saved.length, 0);
  h.timeline.visible(["m0039", "m0040", "m0041"], foreground);
  assert.equal(h.timeline.getState().target, undefined);
  assert.deepEqual(h.saved.at(-1), anchorOf(39));
});

test("a deleted message shows as deleted at once, even with a read in flight", async () => {
  const queries = new FakeConversation(10);
  const h = harness(queries);
  await h.timeline.load();
  let release = (): void => {};
  queries.gate = new Promise((resolve) => {
    release = resolve;
  });
  const refresh = h.timeline.refresh();
  await settle();
  h.timeline.messageDeleted("m0010");
  release();
  await refresh;
  const row = h.timeline.getState().messages.find((m) => m.id === "m0010");
  assert.deepEqual(row?.content, { kind: "deleted" });
  assert.equal(h.timeline.getState().loading, false);
});

test("a failed read shows Message updates stopped", async () => {
  const queries = new FakeConversation(3);
  queries.historyError = new Error("offline");
  const h = harness(queries);
  await h.timeline.load();
  assert.equal(h.timeline.getState().error, "Message updates stopped.");
  queries.historyError = undefined;
  await h.timeline.refresh();
  assert.equal(h.timeline.getState().error, undefined);
  assert.equal(h.timeline.getState().messages.length, 3);
});

test("a closed chat ignores results", async () => {
  const queries = new FakeConversation(3);
  const h = harness(queries);
  let release = (): void => {};
  queries.gate = new Promise((resolve) => {
    release = resolve;
  });
  const load = h.timeline.load();
  h.open = false;
  release();
  await load;
  assert.equal(h.timeline.getState().messages.length, 0);
  assert.equal(h.timeline.getState().loaded, false);
});

test("a newer action supersedes an older one", async () => {
  const queries = new FakeConversation(135);
  const h = harness(queries);
  await h.timeline.load();
  let release = (): void => {};
  queries.gate = new Promise((resolve) => {
    release = resolve;
  });
  const older = h.timeline.older();
  await settle();
  queries.gate = undefined;
  await h.timeline.newest();
  release();
  await older;
  assert.equal(h.timeline.getState().messages.length, 50);
});

test("a cached window shows at once when the chat reopens", async () => {
  const cache = new TranscriptCache();
  const first = harness(new FakeConversation(5), { cache });
  await first.timeline.load();
  const second = harness(new FakeConversation(5), { cache });
  assert.equal(second.timeline.getState().loaded, true);
  assert.equal(second.timeline.getState().messages.length, 5);
});
