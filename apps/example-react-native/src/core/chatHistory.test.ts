import assert from "node:assert/strict";
import { test } from "node:test";

import { CONVERSATION, FakeConversation, textMessage } from "./chatFakes.ts";
import {
  type HistoryWindow,
  TranscriptCache,
  anchorTarget,
  aroundWindow,
  canMarkRead,
  olderWindow,
  readMarker,
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

test("older pages page backwards without duplicates through 135 messages", async () => {
  const queries = new FakeConversation(135);
  const newest = await olderWindow(queries, CONVERSATION);
  assert.equal(newest.rows.length, 50);
  assert.equal(newest.rows[0]?.id, "m0135");
  assert.equal(newest.atNewest, true);
  assert.equal(newest.hasOlder, true);
  const second = await olderWindow(queries, CONVERSATION, newest.last);
  assert.equal(second.rows[0]?.id, "m0085");
  assert.equal(second.atNewest, false);
  const third = await olderWindow(queries, CONVERSATION, second.last);
  assert.equal(third.rows.length, 35);
  assert.equal(third.hasOlder, false);
  const all = [...newest.rows, ...second.rows, ...third.rows];
  assert.equal(new Set(ids(all)).size, 135);
});

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

test("an older page appends to the cached window", () => {
  const cache = new TranscriptCache();
  cache.put(CONVERSATION, window(newestFirst(135, 86), { hasOlder: true }));
  const appended = cache.append(
    CONVERSATION,
    window(newestFirst(85, 36), { hasOlder: true, atNewest: false }),
    "m0086",
  );
  assert.equal(appended.rows.length, 100);
  assert.equal(appended.atNewest, true);
  assert.equal(appended.first?.cursor, "m0135");
  assert.equal(appended.last?.cursor, "m0036");
});

test("an older page appends to the window on screen after an eviction", () => {
  const cache = new TranscriptCache();
  const onScreen = window(newestFirst(135, 86), { hasOlder: true });
  const appended = cache.append(
    CONVERSATION,
    window(newestFirst(85, 36), { atNewest: false }),
    "m0086",
    onScreen,
  );
  assert.equal(appended.rows[0]?.id, "m0135");
  assert.equal(appended.rows.length, 100);
});

test("a refresh replaces the overlap and keeps older history", () => {
  const cache = new TranscriptCache();
  cache.put(CONVERSATION, window(newestFirst(100, 1)));
  const fresh = newestFirst(102, 53).map((row) =>
    row.id === "m0060"
      ? { ...row, content: { kind: "deleted" as const } }
      : row,
  );
  const merged = cache.refresh(CONVERSATION, window(fresh, { hasOlder: true }));
  assert.equal(merged.rows.length, 102);
  assert.equal(merged.rows[0]?.id, "m0102");
  assert.equal(merged.rows.at(-1)?.id, "m0001");
  assert.equal(
    merged.rows.find((row) => row.id === "m0060")?.content.kind,
    "deleted",
  );
  assert.equal(merged.hasOlder, false);
});

test("a deleted cached message cannot render its old body", () => {
  const cache = new TranscriptCache();
  const row = { ...textMessage(1), reactions: [] };
  cache.put(CONVERSATION, window([row]));
  cache.markDeleted(CONVERSATION, row.id);
  assert.deepEqual(cache.get(CONVERSATION)?.rows[0]?.content, {
    kind: "deleted",
  });
});

const anchorOf = (n: number) => {
  const row = textMessage(n);
  assert.ok(row.position !== undefined);
  return { messageId: row.id, position: row.position };
};

test("anchor restore: the anchor row when it survives", () => {
  assert.deepEqual(anchorTarget(anchorOf(5), newestFirst(10, 1)), {
    id: "m0005",
    changed: false,
  });
});

test("anchor restore: a deleted anchor moves to the nearest newer survivor", () => {
  const rows = newestFirst(10, 1).map((row) =>
    row.id === "m0005" || row.id === "m0006"
      ? { ...row, content: { kind: "deleted" as const } }
      : row,
  );
  assert.deepEqual(anchorTarget(anchorOf(5), rows), {
    id: "m0007",
    changed: true,
  });
  const newestDeleted = newestFirst(5, 1).map((row) =>
    row.id === "m0005"
      ? { ...row, content: { kind: "deleted" as const } }
      : row,
  );
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

test("the timeline shows published rows, then pending ones, oldest first, without reactions", () => {
  const reaction: ChatMessage = {
    ...textMessage(4),
    content: { kind: "reaction" },
  };
  const pending: ChatMessage = { ...textMessage(9), delivery: "queued" };
  const older: ChatMessage = { ...textMessage(8), delivery: "failed" };
  assert.deepEqual(
    ids(
      timelineMessages(window([textMessage(5), reaction, textMessage(3)]), [
        pending,
        older,
      ]),
    ),
    ["m0003", "m0005", "m0008", "m0009"],
  );
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

test("read markers need foreground, current, newest page and newest row", () => {
  const all = {
    foreground: true,
    current: true,
    windowAtNewest: true,
    newestVisible: true,
  };
  assert.equal(canMarkRead(all), true);
  for (const key of Object.keys(all) as (keyof typeof all)[]) {
    assert.equal(canMarkRead({ ...all, [key]: false }), false, key);
  }
  assert.equal(
    readMarker(window([textMessage(3), textMessage(7)])),
    7_000_000_001n,
  );
  assert.equal(readMarker(window([])), undefined);
});
