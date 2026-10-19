import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CONVERSATION,
  FakeConversation,
  OWN,
  PAGE_SIZE,
  anchorOf,
  reactionRecord,
  textMessage,
} from "./chatFakes.ts";
import {
  type ReadingAnchor,
  TimelineCopy,
  TranscriptCache,
} from "./chatHistory.ts";
import type { ChatMessage, ConversationRecord } from "./chatMessages.ts";
import { ChatTimeline, PENDING_READ_ATTEMPTS } from "./chatTimeline.ts";
import { settle } from "./testing.ts";

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

const visibleIds = (h: Harness, from: number, to?: number) =>
  h.timeline
    .getState()
    .messages.slice(from, to)
    .map((row) => row.id);

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

test("a stream refresh keeps loaded history, adds the new message and re-reads changed rows", async () => {
  const queries = new FakeConversation(100);
  const h = harness(queries);
  await h.timeline.load();
  await h.timeline.older();
  queries.redact("m0090");
  queries.published.push(textMessage(101));
  await h.timeline.refresh();
  const { messages, window } = h.timeline.getState();
  assert.equal(messages.length, 101);
  assert.equal(messages[0]?.id, "m0001");
  assert.equal(messages.at(-1)?.id, "m0101");
  assert.deepEqual(messages.find((row) => row.id === "m0090")?.content, {
    kind: "deleted",
  });
  // All history is still loaded, so "Older messages" stays hidden.
  assert.equal(window.hasOlder, false);
});

test("reopening restores the saved anchor in reading mode until the target shows", async () => {
  const h = harness(new FakeConversation(135), { anchor: anchorOf(40) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, "m0040");
  assert.equal(state.targetRevision, 1);
  assert.equal(state.atNewest, false);
  assert.equal(state.window.notice, undefined);
  assert.equal(state.window.atNewest, false);
  assert.equal(state.window.hasOlder, false);
  assert.ok(state.messages.some((row) => row.id === "m0040"));
  // The list first reports its bottom rows; the reading position stays.
  h.timeline.visible(["m0089", "m0090"], foreground);
  assert.equal(h.timeline.getState().target, "m0040");
  assert.equal(h.saved.length, 0);
  h.timeline.visible(["m0039", "m0040", "m0041"], foreground);
  assert.equal(h.timeline.getState().target, undefined);
  assert.deepEqual(h.saved.at(-1), anchorOf(39));
  // The newest loaded row is not the newest message: no read marker.
  h.timeline.visible(visibleIds(h, -3), foreground);
  assert.equal(h.marks.length, 0);
});

test("a deleted anchor restores to the nearest surviving message with Position changed", async () => {
  const queries = new FakeConversation(135);
  queries.redact("m0040");
  queries.published[40] = { ...textMessage(41), content: { kind: "reaction" } };
  const h = harness(queries, { anchor: anchorOf(40) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, "m0042");
  assert.equal(state.window.notice, TimelineCopy.positionChanged);
  assert.equal(state.atNewest, false);
});

test("an anchor without survivors near it falls back to the newest page", async () => {
  const queries = new FakeConversation(200);
  for (let n = 1; n <= 150; n += 1) queries.redact(textMessage(n).id);
  const h = harness(queries, { anchor: anchorOf(20) });
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.target, undefined);
  assert.equal(state.window.notice, TimelineCopy.positionChanged);
  assert.equal(state.window.atNewest, true);
  assert.equal(state.messages.at(-1)?.id, "m0200");
});

test("visibility saves the topmost surviving row once, and unchanged frames change nothing", async () => {
  const queries = new FakeConversation(10);
  queries.redact("m0003");
  const h = harness(queries);
  await h.timeline.load();
  h.timeline.visible(visibleIds(h, 2, 6), foreground);
  assert.deepEqual(h.saved, [anchorOf(4)]);
  assert.equal(h.timeline.getState().anchorId, "m0004");
  assert.equal(h.timeline.getState().atNewest, false);
  let changes = 0;
  h.timeline.subscribe(() => {
    changes += 1;
  });
  // Same topmost row, newest row still hidden: scrolling frames re-render and save nothing.
  h.timeline.visible(visibleIds(h, 2, 6), foreground);
  h.timeline.visible(visibleIds(h, 2, 7), foreground);
  assert.equal(changes, 0);
  assert.equal(h.saved.length, 1);
  h.timeline.visible(visibleIds(h, 4, 10), foreground);
  assert.ok(changes > 0);
  assert.equal(h.timeline.getState().anchorId, "m0005");
  assert.equal(h.timeline.getState().atNewest, true);
});

test("the read marker advances only in the foreground, current, at the newest row", async () => {
  const queries = new FakeConversation(10);
  const late = textMessage(10).insertedAtNs + 5n;
  queries.published[3] = { ...textMessage(4), insertedAtNs: late };
  const h = harness(queries);
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
  assert.equal(h.marks[0]?.marker, late);
  assert.equal(h.timeline.getState().atNewest, true);
  // The next scrolling frame does not write the same marker again.
  h.timeline.visible(visibleIds(h, 5, 10), foreground);
  assert.equal(h.marks.length, 1);
});

test("a deleted message shows as deleted at once, without reactions, even with a read in flight", async () => {
  const queries = new FakeConversation(5);
  queries.published[4] = {
    ...textMessage(5),
    reactions: [reactionRecord(OWN, "👍", 6)],
  };
  const cache = new TranscriptCache();
  const h = harness(queries, { cache });
  await h.timeline.load();
  assert.equal(h.timeline.getState().messages.at(-1)?.reactions.length, 1);
  const release = queries.hold();
  const refresh = h.timeline.refresh();
  h.timeline.messageDeleted("m0005");
  release();
  await refresh;
  const state = h.timeline.getState();
  const row = state.messages.find((m) => m.id === "m0005");
  assert.deepEqual(row?.content, { kind: "deleted" });
  assert.deepEqual(row?.reactions, []);
  assert.equal(state.loading, false);
  const cached = cache.get(CONVERSATION)?.rows.find((r) => r.id === "m0005");
  assert.deepEqual(cached?.content, { kind: "deleted" });
  assert.deepEqual(cached?.reactions, []);
});

test("a failed read shows Message updates stopped", async (t) => {
  t.mock.method(console, "warn", () => {});
  const queries = new FakeConversation(3);
  queries.historyError = new Error("offline");
  const h = harness(queries);
  await h.timeline.load();
  assert.equal(h.timeline.getState().error, TimelineCopy.updatesStopped);
  queries.historyError = undefined;
  await h.timeline.refresh();
  assert.equal(h.timeline.getState().error, undefined);
  assert.equal(h.timeline.getState().messages.length, 3);
});

test("a closed chat ignores results", async () => {
  const queries = new FakeConversation(3);
  const h = harness(queries);
  const release = queries.hold();
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
  const release = queries.hold();
  const older = h.timeline.older();
  await settle();
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

test("Jump to latest drops a pending restore, so the newest rows report again", async () => {
  const h = harness(new FakeConversation(135), { anchor: anchorOf(40) });
  await h.timeline.load();
  assert.equal(h.timeline.getState().target, "m0040");
  await h.timeline.newest();
  const state = h.timeline.getState();
  assert.equal(state.target, undefined);
  assert.equal(state.atNewest, true);
  assert.equal(state.window.atNewest, true);
  h.timeline.visible(visibleIds(h, -5), foreground);
  assert.deepEqual(h.saved.at(-1), anchorOf(131));
  assert.equal(h.marks.length, 1);
  assert.equal(h.marks[0]?.marker, textMessage(135).insertedAtNs);
});

test("a refresh while following stays at the newest row when the anchor was deleted", async () => {
  const queries = new FakeConversation(10);
  const h = harness(queries);
  await h.timeline.load();
  h.timeline.visible(visibleIds(h, 4, 10), foreground);
  assert.equal(h.timeline.getState().anchorId, "m0005");
  assert.equal(h.timeline.getState().atNewest, true);
  queries.redact("m0005");
  queries.published.push(textMessage(11));
  await h.timeline.refresh();
  const state = h.timeline.getState();
  assert.equal(state.anchorId, "m0006");
  assert.equal(state.target, undefined);
  assert.equal(state.atNewest, true);
  assert.equal(state.messages.at(-1)?.id, "m0011");
});

test("unreadable history rows are skipped with the Load more notice", async () => {
  const queries = new FakeConversation(120);
  // The whole newest page is unreadable: the read continues to the next page.
  for (let n = 71; n <= 120; n += 1) queries.unreadable.add(textMessage(n).id);
  const h = harness(queries);
  await h.timeline.load();
  const state = h.timeline.getState();
  assert.equal(state.window.notice, TimelineCopy.historySkipped);
  assert.equal(state.messages.length, 50);
  assert.equal(state.messages.at(-1)?.id, "m0070");
  assert.equal(state.error, undefined);
});

/**
 * `count` failed own rows, sent after `start` seconds, so after every
 * published one.
 */
function failedRows(count: number, start = 1000): ChatMessage[] {
  const rows: ChatMessage[] = [];
  for (let n = 1; n <= count; n += 1) {
    rows.push({
      ...textMessage(start + n, { sender: OWN }),
      delivery: "failed",
    });
  }
  return rows;
}

/** Adds `rows` to the outbox as rows that cannot be decoded. */
function addUnreadable(
  queries: FakeConversation,
  rows: readonly ChatMessage[],
): void {
  for (const row of rows) {
    queries.unpublished.push(row);
    queries.unreadable.add(row.id);
  }
}

/** One full unreadable page for each pending read a single load makes. */
const unreadableLoad = (start: number) =>
  failedRows(PENDING_READ_ATTEMPTS * PAGE_SIZE, start);

test("unreadable pending rows ask to load more, or to refresh on the last page", async () => {
  const more = new FakeConversation(3);
  more.unpublished.push(...failedRows(60));
  more.unreadable.add("m1060");
  const first = harness(more);
  await first.timeline.load();
  assert.equal(first.timeline.getState().morePending, true);
  assert.equal(
    first.timeline.getState().error,
    TimelineCopy.pendingSkippedMore,
  );
  // The last page reads cleanly, but the loaded rows still miss one.
  await first.timeline.olderPending();
  assert.equal(first.timeline.getState().morePending, false);
  assert.equal(
    first.timeline.getState().error,
    TimelineCopy.pendingSkippedRefresh,
  );

  const last = new FakeConversation(3);
  last.unpublished.push(...failedRows(3));
  last.unreadable.add("m1002");
  const second = harness(last);
  await second.timeline.load();
  const state = second.timeline.getState();
  assert.equal(state.morePending, false);
  assert.equal(state.error, TimelineCopy.pendingSkippedRefresh);
  assert.deepEqual(
    state.messages.slice(-2).map((row) => row.id),
    ["m1001", "m1003"],
  );
});

test("older pending pages add to the pending rows, and a refresh reads the newest again", async () => {
  const queries = new FakeConversation(3);
  const rows = failedRows(120);
  queries.unpublished.push(...rows);
  const h = harness(queries);
  await h.timeline.load();
  assert.equal(h.timeline.getState().morePending, true);
  for (
    let page = 0;
    page < rows.length && h.timeline.getState().morePending;
    page += 1
  ) {
    await h.timeline.olderPending();
  }
  assert.equal(h.timeline.getState().morePending, false);
  const published = queries.published.map((row) => row.id);
  const pending = rows.map((row) => row.id);
  assert.deepEqual(visibleIds(h, 0), [...published, ...pending]);

  // The newest failed row leaves the outbox; the refresh drops it.
  queries.unpublished.pop();
  await h.timeline.refresh();
  const shown = visibleIds(h, 0);
  assert.equal(shown.includes(pending.at(-1) ?? ""), false);
  assert.equal(new Set(shown).size, shown.length);
});

test("a refresh past only unreadable pending pages drops the old rows, so paging shows them once", async () => {
  const queries = new FakeConversation(3);
  const rows = failedRows(10);
  queries.unpublished.push(...rows);
  const h = harness(queries);
  await h.timeline.load();
  // Every newer page that one refresh reads is unreadable.
  addUnreadable(queries, unreadableLoad(5000));
  await h.timeline.refresh();
  assert.equal(h.timeline.getState().morePending, true);
  assert.equal(h.timeline.getState().error, TimelineCopy.pendingSkippedMore);
  for (
    let page = 0;
    page < queries.unpublished.length && h.timeline.getState().morePending;
    page += 1
  ) {
    await h.timeline.olderPending();
  }
  const published = queries.published.map((row) => row.id);
  assert.deepEqual(visibleIds(h, 0), [
    ...published,
    ...rows.map((row) => row.id),
  ]);
  // The rows the refresh skipped still count on the last page.
  assert.equal(h.timeline.getState().morePending, false);
  assert.equal(h.timeline.getState().error, TimelineCopy.pendingSkippedRefresh);
});

test("older pending pages past only unreadable rows keep the newer pending rows", async () => {
  const queries = new FakeConversation(3);
  const older = failedRows(10);
  queries.unpublished.push(...older);
  const unreadable = unreadableLoad(5000);
  addUnreadable(queries, unreadable);
  const newer = failedRows(PAGE_SIZE, 5000 + unreadable.length);
  queries.unpublished.push(...newer);
  const h = harness(queries);
  await h.timeline.load();
  const published = queries.published.map((row) => row.id);
  const ids = (rows: readonly ChatMessage[]) => rows.map((row) => row.id);
  assert.deepEqual(visibleIds(h, 0), [...published, ...ids(newer)]);

  // This load reads only unreadable pages and stops before the older rows.
  await h.timeline.olderPending();
  assert.equal(h.timeline.getState().morePending, true);
  assert.equal(h.timeline.getState().error, TimelineCopy.pendingSkippedMore);
  assert.deepEqual(visibleIds(h, 0), [...published, ...ids(newer)]);

  await h.timeline.olderPending();
  assert.deepEqual(visibleIds(h, 0), [
    ...published,
    ...ids(older),
    ...ids(newer),
  ]);
});
