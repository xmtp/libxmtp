import assert from "node:assert/strict";
import { test } from "node:test";

import type { MessageContent } from "@xmtp/react-native-sdk";

import {
  type ConversationConsent,
  type ConversationListPreferences,
  ConversationListStore,
  type ConversationQueries,
  type ConversationRecord,
  LOAD_FAILED,
  PreviewCopy,
  conversationRowLabel,
  previewText,
} from "./conversationList.ts";
import { timeOfDay } from "./dates.ts";
import { CONVERSATION_PAGE_SIZE, REQUEST_COUNT_LIMIT } from "./requests.ts";
import { settle } from "./testing.ts";

const group = (
  id: string,
  consent: ConversationConsent = "allowed",
): ConversationRecord => ({
  id,
  kind: "group",
  consent,
  preview: "",
  unread: 0,
});

/** A fake SDK: `total` conversations per consent, details named after the ID. */
class FakeQueries implements ConversationQueries {
  readonly lists: { consent: ConversationConsent; limit: number }[] = [];
  readonly details_: string[] = [];
  totals: Partial<Record<ConversationConsent, number>>;
  failList = false;
  failCount = false;
  /** Overrides the consent that details report, by ID. */
  readonly consentOf = new Map<string, ConversationConsent>();
  markers: bigint[] = [];

  constructor(totals: Partial<Record<ConversationConsent, number>>) {
    this.totals = totals;
  }

  list(
    consent: ConversationConsent,
    limit: number,
  ): Promise<readonly ConversationRecord[]> {
    this.lists.push({ consent, limit });
    if (this.failList && consent !== "unknown") {
      return Promise.reject(new Error("list"));
    }
    if (this.failCount && consent === "unknown") {
      return Promise.reject(new Error("count"));
    }
    const total = this.totals[consent] ?? 0;
    return Promise.resolve(
      Array.from({ length: Math.min(total, limit) }, (_, index) =>
        group(String(index), consent),
      ),
    );
  }

  details(
    record: ConversationRecord,
    marker: (record: ConversationRecord) => bigint,
  ): Promise<ConversationRecord> {
    this.details_.push(record.id);
    this.markers.push(marker(record));
    return Promise.resolve({
      ...record,
      name: `Group ${record.id}`,
      preview: `preview ${record.id}`,
      unread: 1,
      consent: this.consentOf.get(record.id) ?? record.consent,
    });
  }
}

class FakePreferences implements ConversationListPreferences {
  dismissedRequests: ReadonlySet<string>;
  readonly aliases = new Map<string, string>();
  readonly markers = new Map<string, bigint>();

  constructor(dismissed: Iterable<string> = []) {
    this.dismissedRequests = new Set(dismissed);
  }

  alias(id: string): string | undefined {
    return this.aliases.get(id);
  }

  marker(key: string): bigint {
    return this.markers.get(key) ?? 0n;
  }
}

function store(
  consent: ConversationConsent,
  queries: ConversationQueries,
  preferences: ConversationListPreferences = new FakePreferences(),
  isCurrent: () => boolean = () => true,
): ConversationListStore {
  return new ConversationListStore({
    consent,
    ownInboxId: "own",
    queries,
    preferences,
    isCurrent,
  });
}

const range = (from: number, to: number): string[] =>
  Array.from({ length: to - from }, (_, index) => String(from + index));

/** Detail reads that finish only when the test says so, numbered by call. */
class HeldDetails {
  readonly base: FakeQueries;
  readonly held: (() => void)[] = [];
  calls = 0;
  running = 0;
  peak = 0;

  constructor(base: FakeQueries) {
    this.base = base;
  }

  readonly queries: ConversationQueries = {
    list: (consent, limit) => this.base.list(consent, limit),
    details: (record) => {
      this.calls += 1;
      const name = `${record.id} v${String(this.calls)}`;
      this.running += 1;
      this.peak = Math.max(this.peak, this.running);
      return new Promise((resolve) => {
        this.held.push(() => {
          this.running -= 1;
          resolve({ ...record, name });
        });
      });
    },
  };

  /** Finishes the oldest held read. */
  async release(): Promise<void> {
    this.held.shift()?.();
    await settle();
  }

  /** Finishes the newest held read. */
  async releaseNewest(): Promise<void> {
    this.held.pop()?.();
    await settle();
  }

  async releaseAll(): Promise<void> {
    while (this.held.length > 0) await this.release();
  }
}

/** A list whose first read for `allowed` waits until the test settles it. */
function heldFirstList(queries: FakeQueries): {
  readonly queries: ConversationQueries;
  settleFirst(value: readonly ConversationRecord[] | Error): void;
} {
  let settleFirst: (
    value: readonly ConversationRecord[] | Error,
  ) => void = () => {};
  let allowedReads = 0;
  return {
    queries: {
      list: (consent, limit) => {
        if (consent === "allowed" && ++allowedReads === 1) {
          return new Promise((resolve, reject) => {
            settleFirst = (value) => {
              if (value instanceof Error) reject(value);
              else resolve(value);
            };
          });
        }
        return queries.list(consent, limit);
      },
      details: (record, marker) => queries.details(record, marker),
    },
    settleFirst: (value) => {
      settleFirst(value);
    },
  };
}

const page = CONVERSATION_PAGE_SIZE;
for (const { name, dismissed, hidden } of [
  { name: "the first page", dismissed: range(0, page), hidden: page },
  {
    name: "the first two pages",
    dismissed: range(0, 2 * page),
    hidden: 2 * page,
  },
  // Dismissals outside the list pad the read but do not widen the page.
  {
    name: "requests outside the list",
    dismissed: range(1000, 1000 + page),
    hidden: 0,
  },
]) {
  test(`Requests shows full pages when ${name} are dismissed`, async () => {
    // `hidden` dismissed requests come first, then a page and a half of others.
    const queries = new FakeQueries({ unknown: hidden + page + page / 2 });
    const list = store("unknown", queries, new FakePreferences(dismissed));
    await list.refresh();
    let state = list.getState();
    assert.equal(state.rows.length, page);
    assert.equal(state.rows[0]?.id, String(hidden));
    assert.ok(state.hasMore);
    list.loadMore();
    await list.whenIdle();
    state = list.getState();
    assert.equal(state.rows.length, page + page / 2);
    assert.equal(state.rows.at(-1)?.id, String(hidden + page + page / 2 - 1));
    assert.ok(!state.hasMore);
  });
}

test("only Chats reads the request badge: it counts requests below the limit without dismissed ones", async () => {
  const shown = REQUEST_COUNT_LIMIT - 1;
  const queries = new FakeQueries({ allowed: 2, unknown: shown });
  const list = store("allowed", queries, new FakePreferences(["3", "4"]));
  await list.refresh();
  assert.equal(list.getState().requestCount, shown - 2);
  queries.totals = { allowed: 2, unknown: REQUEST_COUNT_LIMIT };
  await list.refresh();
  // The limit counts the dismissed ones too.
  assert.equal(list.getState().requestCount, undefined);

  const requests = new FakeQueries({ unknown: 3 });
  const requestList = store("unknown", requests);
  await requestList.refresh();
  assert.equal(requests.lists.length, 1, "Requests reads its list once");
  assert.equal(requestList.getState().requestCount, undefined);
});

test("a failed request count hides the number but keeps the list", async () => {
  const queries = new FakeQueries({ allowed: 2, unknown: 3 });
  queries.failCount = true;
  const list = store("allowed", queries);
  await list.refresh();
  assert.equal(list.getState().requestCount, undefined);
  assert.equal(list.getState().rows.length, 2);
  assert.equal(list.getState().error, undefined);
});

test("details fill the visible window plus 50 rows, not the whole list", async () => {
  const queries = new FakeQueries({ allowed: 120 });
  const list = store("allowed", queries);
  list.load();
  await list.whenIdle();
  // First page: 50 rows, window 0..60 clamped to the 50 loaded.
  assert.equal(queries.details_.length, 50);
  list.visible(47, 1);
  await list.whenIdle();
  // Row 47 is within 3 of the end: the second page loads and reads the window 47..98.
  const state = list.getState();
  assert.equal(state.rows.length, 100);
  assert.equal(state.rows[47]?.title, "Group 47");
  assert.equal(state.rows[97]?.title, "Group 97");
  assert.equal(state.rows[98]?.title, "Group");
  assert.equal(state.rows[47]?.record.preview, "preview 47");
});

test("a reload keeps known details until the window reads them again", async () => {
  const held = new HeldDetails(new FakeQueries({ allowed: 1 }));
  const list = store("allowed", held.queries);
  list.load();
  await settle();
  await held.releaseAll();
  assert.equal(list.getState().rows[0]?.title, "0 v1");
  list.load();
  await settle();
  // The list read finished; the row's second detail read is still held.
  assert.equal(held.held.length, 1);
  assert.equal(list.getState().rows[0]?.title, "0 v1");
  await held.releaseAll();
  assert.equal(list.getState().rows[0]?.title, "0 v2");
});

test("a row whose consent changed leaves the list when its details arrive", async () => {
  const queries = new FakeQueries({ allowed: 3 });
  queries.consentOf.set("1", "denied");
  const list = store("allowed", queries);
  await list.refresh();
  assert.deepEqual(
    list.getState().rows.map((row) => row.id),
    ["0", "2"],
  );
});

test("search filters loaded rows by title, handle, preview and ID, ignoring case", async () => {
  const queries = new FakeQueries({ allowed: 12 });
  const list = store("allowed", queries);
  await list.refresh();
  list.setSearch("PREVIEW 1");
  await list.whenIdle();
  assert.deepEqual(
    list.getState().visible.map((row) => row.id),
    ["1", "10", "11"],
  );
  assert.equal(list.getState().rows.length, 12);
  list.setSearch("");
  assert.equal(list.getState().visible.length, 12);
});

test("search reads unread rows of the loaded window so they can match", async () => {
  const queries = new FakeQueries({ allowed: 5 });
  const list = store("allowed", queries);
  list.load();
  // Search before any details arrived: titles are still "Group".
  list.setSearch("Group 3");
  await list.whenIdle();
  assert.deepEqual(
    list.getState().visible.map((row) => row.id),
    ["3"],
  );
});

test("paging stops while searching", async () => {
  const queries = new FakeQueries({ allowed: 120 });
  const list = store("allowed", queries);
  await list.refresh();
  list.setSearch("Group");
  list.visible(49, 1);
  await list.whenIdle();
  assert.equal(list.getState().rows.length, 50);
});

test("a failed load shows the error", async () => {
  const queries = new FakeQueries({ allowed: 2 });
  queries.failList = true;
  const list = store("allowed", queries);
  await list.refresh();
  assert.equal(list.getState().error, LOAD_FAILED);
  assert.ok(!list.getState().loading);
});

test("a superseded list read cannot change a newer load", async () => {
  for (const late of [
    [group("old-a"), group("old-b"), group("old-c")],
    new Error("late"),
  ]) {
    const label = late instanceof Error ? "late failure" : "late result";
    const slow = heldFirstList(new FakeQueries({ allowed: 2 }));
    const list = store("allowed", slow.queries);
    const first = list.refresh();
    await list.refresh();
    slow.settleFirst(late);
    await first;
    assert.deepEqual(
      list.getState().rows.map((row) => row.id),
      ["0", "1"],
      label,
    );
    assert.equal(list.getState().error, undefined, label);
  }
});

test("results after the session ended or the store closed are dropped", async () => {
  let current = true;
  const queries = new FakeQueries({ allowed: 2 });
  const list = store("allowed", queries, new FakePreferences(), () => current);
  const pending = list.refresh();
  current = false;
  await pending;
  assert.equal(list.getState().rows.length, 0);
  assert.ok(!list.getState().loaded);

  const closed = store("allowed", queries);
  const loading = closed.refresh();
  closed.dispose();
  await loading;
  assert.equal(closed.getState().rows.length, 0);
});

test("a row takes its local alias and counts unread after its read marker", async () => {
  const queries = new FakeQueries({ allowed: 1 });
  const preferences = new FakePreferences();
  preferences.aliases.set("0", "Book club");
  preferences.markers.set("0", 42n);
  const list = store("allowed", queries, preferences);
  await list.refresh();
  assert.equal(list.getState().rows[0]?.title, "Book club");
  assert.equal(list.getState().rows[0]?.initials, "BC");
  assert.deepEqual(queries.markers, [42n]);
});

test("a row previews text, a reply's body or fallback, or the attachment label", () => {
  const text: MessageContent = { kind: "text", value: "Hi" };
  assert.equal(previewText(text, "x"), "Hi");
  assert.equal(previewText({ kind: "markdown", value: "**b**" }, "x"), "**b**");
  const reply = (body: MessageContent): MessageContent =>
    ({ kind: "reply", referenceId: "m", body }) as MessageContent;
  assert.equal(previewText(reply({ kind: "text", value: "Yes" }), "f"), "Yes");
  assert.equal(previewText(reply({ kind: "readReceipt" }), "f"), "f");
  assert.equal(
    previewText(reply({ kind: "readReceipt" }), undefined),
    PreviewCopy.reply,
  );
  assert.equal(
    previewText(
      { kind: "remoteAttachment", value: {} } as MessageContent,
      "file",
    ),
    PreviewCopy.attachment,
  );
  assert.equal(previewText({ kind: "readReceipt" }, "fallback"), "fallback");
  assert.equal(previewText({ kind: "readReceipt" }, undefined), "");
});

for (const end of ["closed", "session ended"] as const) {
  test(`details that arrive after the store ${end} are dropped`, async () => {
    let current = true;
    const held = new HeldDetails(new FakeQueries({ allowed: 1 }));
    const list = store(
      "allowed",
      held.queries,
      new FakePreferences(),
      () => current,
    );
    list.load();
    await settle();
    assert.equal(held.held.length, 1);
    if (end === "closed") list.dispose();
    else current = false;
    await held.release();
    assert.equal(list.getState().rows[0]?.title, "Group");
  });
}

test("details from an earlier load's window cannot overwrite the newer window's", async () => {
  const held = new HeldDetails(new FakeQueries({ allowed: 1 }));
  const list = store("allowed", held.queries);
  list.load();
  await settle();
  // A stream update reloads while the first read (v1) still runs: the new
  // window reads the row again (v2), and v2 finishes first.
  list.load();
  await settle();
  assert.equal(held.calls, 2);
  await held.releaseNewest();
  assert.equal(list.getState().rows[0]?.title, "0 v2");
  await held.release();
  await list.whenIdle();
  assert.equal(list.getState().rows[0]?.title, "0 v2");
});

test("reloads never add detail reads beyond the limit", async () => {
  const held = new HeldDetails(new FakeQueries({ allowed: 20 }));
  const list = store("allowed", held.queries);
  list.load();
  await settle();
  // The limit is what the first load fills.
  const limit = held.running;
  assert.ok(limit > 0 && limit < 20, String(limit));
  list.load();
  await settle();
  list.load();
  await settle();
  assert.equal(held.peak, limit);
  await held.releaseAll();
  await list.whenIdle();
  assert.equal(held.peak, limit);
  assert.equal(list.getState().rows[19]?.title?.startsWith("19 v"), true);
});

test("loading more while a load runs reads nothing extra", async () => {
  const queries = new FakeQueries({ allowed: 120 });
  const list = store("allowed", queries);
  list.load();
  list.loadMore();
  await list.whenIdle();
  assert.equal(
    queries.lists.filter((read) => read.consent === "allowed").length,
    1,
  );
  assert.equal(list.getState().rows.length, 50);
});

test("a row's accessibility label says its title, time, preview and unread count", () => {
  const at = new Date(2026, 9, 18, 13, 5);
  const dm = {
    id: "d",
    record: {
      ...group("d"),
      kind: "dm" as const,
      peerHandle: "lena#0935",
      preview: "See you",
      unread: 3,
      lastActivity: at,
    },
    title: "@lena#0935",
    initials: "L",
  };
  const parts = conversationRowLabel(dm).split(", ");
  assert.ok(parts.includes("lena#0935"), parts.join(", "));
  assert.ok(parts.includes(timeOfDay(at)), parts.join(", "));
  assert.ok(parts.includes("See you"), parts.join(", "));
  assert.equal(parts.at(-1), "3", "the unread count comes last");
  // A read row with no preview leaves both out.
  const read = { ...dm, record: { ...dm.record, unread: 0, preview: "" } };
  const readParts = conversationRowLabel(read).split(", ");
  assert.equal(readParts.at(-1), timeOfDay(at), readParts.join(", "));
  assert.ok(readParts.every((part) => part !== "" && part !== "0"));
  const weekend = {
    id: "g",
    record: { ...group("g"), name: "Weekend plans" },
    title: "Weekend plans",
    initials: "WP",
  };
  assert.ok(conversationRowLabel(weekend).startsWith("Weekend plans"));
});
