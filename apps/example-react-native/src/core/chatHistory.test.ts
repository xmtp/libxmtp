import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CONVERSATION,
  FakeConversation,
  anchorOf,
  deletedMessage,
  textMessage,
} from "./chatFakes.ts";
import {
  type HistoryWindow,
  TranscriptCache,
  anchorTarget,
  aroundWindow,
  showsDateSeparator,
  timelineMessages,
} from "./chatHistory.ts";
import type { ChatMessage } from "./chatMessages.ts";

const ids = (rows: readonly ChatMessage[]) => rows.map((row) => row.id);

/** Rows newest first, numbered `from` down to `to`. */
function newestFirst(from: number, to: number): ChatMessage[] {
  const rows: ChatMessage[] = [];
  for (let n = from; n >= to; n -= 1) rows.push(textMessage(n));
  return rows;
}

function window(rows: ChatMessage[], extra: Partial<HistoryWindow> = {}) {
  return {
    rows,
    first: rows[0]?.position,
    last: rows.at(-1)?.position,
    hasOlder: false,
    atNewest: true,
    ...extra,
  } as HistoryWindow;
}

test("around reads newer rows after the anchor, then older ones before them", async () => {
  const queries = new FakeConversation(135);
  const anchor = textMessage(60).position;
  assert.ok(anchor !== undefined);
  const around = await aroundWindow(queries, CONVERSATION, anchor);
  // One ascending page after 60 (61…110), then one descending page before 61.
  assert.equal(around.rows[0]?.id, "m0110");
  assert.equal(around.rows.length, 100);
  assert.equal(around.atNewest, false);
  assert.equal(around.first?.cursor, "m0110");
  assert.ok(ids(around.rows).includes("m0060"));
  assert.equal(new Set(ids(around.rows)).size, around.rows.length);
  assert.equal(around.rows.at(-1)?.id, "m0011");
  assert.equal(around.hasOlder, true);
});

test("the cache keeps 500 rows around the anchor", () => {
  const cache = new TranscriptCache();
  const rows = newestFirst(800, 1);
  const kept = cache.put(CONVERSATION, window(rows), "m0300");
  assert.equal(kept.rows.length, 500);
  // The anchor m0300 sits at index 500; the window starts 250 rows newer.
  assert.equal(kept.rows[0]?.id, "m0550");
  assert.equal(kept.atNewest, false);
  assert.equal(kept.hasOlder, true);
  assert.equal(kept.first?.cursor, "m0550");
  assert.equal(kept.last?.cursor, "m0051");
});

test("the cache drops duplicate rows and keeps three conversations", () => {
  const cache = new TranscriptCache();
  const rows = [textMessage(2), textMessage(2), textMessage(1)];
  assert.deepEqual(ids(cache.put("a", window(rows)).rows), ["m0002", "m0001"]);
  cache.put("b", window([textMessage(1)]));
  cache.put("c", window([textMessage(1)]));
  cache.get("a");
  cache.put("d", window([textMessage(1)]));
  assert.ok(cache.get("a") !== undefined);
  assert.equal(cache.get("b"), undefined);
});

test("an older page appends to the cached window, or to the window on screen after an eviction", () => {
  const onScreen = window(newestFirst(135, 86), { hasOlder: true });
  const page = window(newestFirst(85, 36), { hasOlder: true, atNewest: false });
  const cached = new TranscriptCache();
  cached.put(CONVERSATION, onScreen);
  const evicted = new TranscriptCache();
  for (const appended of [
    cached.append(CONVERSATION, page, "m0086"),
    evicted.append(CONVERSATION, page, "m0086", onScreen),
  ]) {
    assert.equal(appended.rows.length, 100);
    assert.equal(appended.rows[0]?.id, "m0135");
    assert.equal(appended.atNewest, true);
    assert.equal(appended.first?.cursor, "m0135");
    assert.equal(appended.last?.cursor, "m0036");
  }
});

test("a refresh of the newest page keeps an older window off the newest end", () => {
  const cache = new TranscriptCache();
  // A restored window around an anchor, not at the newest end.
  cache.put(
    CONVERSATION,
    window(newestFirst(110, 11), { atNewest: false, hasOlder: true }),
  );
  const merged = cache.refresh(
    CONVERSATION,
    window(newestFirst(135, 86), { hasOlder: true }),
  );
  // Rows newer than the window stay out, so it still is not at the newest end.
  assert.equal(merged.rows[0]?.id, "m0110");
  assert.equal(merged.rows.length, 100);
  assert.equal(merged.atNewest, false);
  assert.equal(merged.first?.cursor, "m0110");
});

test("anchor restore: a deleted anchor moves to the nearest newer survivor", () => {
  const rows = [
    ...newestFirst(10, 7),
    deletedMessage(6),
    deletedMessage(5),
    ...newestFirst(4, 1),
  ];
  assert.deepEqual(anchorTarget(anchorOf(5), rows), {
    id: "m0007",
    changed: true,
  });
  const newestDeleted = [deletedMessage(5), ...newestFirst(4, 1)];
  assert.deepEqual(anchorTarget(anchorOf(5), newestDeleted), {
    id: "m0004",
    changed: true,
  });
});

test("anchor restore: an absent anchor uses the nearest sent time", () => {
  const rows = newestFirst(10, 1).filter((row) => row.id !== "m0005");
  assert.deepEqual(anchorTarget(anchorOf(5), rows), {
    id: "m0006",
    changed: true,
  });
  assert.deepEqual(anchorTarget(anchorOf(11), newestFirst(10, 1)), {
    id: "m0010",
    changed: true,
  });
  assert.deepEqual(anchorTarget(anchorOf(11), []), { changed: true });
});

test("the timeline shows published rows, then pending ones, oldest first, each once, without reactions", () => {
  const reaction: ChatMessage = {
    ...textMessage(4),
    content: { kind: "reaction" },
  };
  const pendingReaction: ChatMessage = {
    ...textMessage(10),
    delivery: "queued",
    content: { kind: "reaction" },
  };
  const queued: ChatMessage = { ...textMessage(9), delivery: "queued" };
  const failed: ChatMessage = { ...textMessage(8), delivery: "failed" };
  // Published since the pending page was read: it shows once, as published.
  const sent = textMessage(6);
  const stale: ChatMessage = { ...sent, delivery: "queued" };
  const rows = timelineMessages(
    window([sent, textMessage(5), reaction, textMessage(3)]),
    [pendingReaction, queued, failed, stale],
  );
  assert.deepEqual(ids(rows), ["m0003", "m0005", "m0006", "m0008", "m0009"]);
  assert.equal(rows[2]?.delivery, "published");
});

test("date separators show on the first row and when the day changes", () => {
  const morning = new Date(2026, 9, 18, 9, 0);
  const evening = new Date(2026, 9, 18, 23, 59);
  const next = new Date(2026, 9, 19, 0, 1);
  assert.equal(showsDateSeparator(undefined, morning), true);
  assert.equal(showsDateSeparator(morning, evening), false);
  assert.equal(showsDateSeparator(evening, next), true);
  assert.equal(
    showsDateSeparator(new Date(2025, 9, 18, 9), new Date(2026, 9, 18, 9)),
    true,
  );
});
