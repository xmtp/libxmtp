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
  filterRows,
  previewText,
} from "./conversationList.ts";

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

for (const dismissed of [50, 100]) {
  test(`a dismissed prefix of ${String(dismissed)} cannot hide older requests (dismissedPrefixCannotHideOlderRequests)`, async () => {
    const queries = new FakeQueries({ unknown: dismissed + 75 });
    const list = store(
      "unknown",
      queries,
      new FakePreferences(range(0, dismissed)),
    );
    await list.refresh();
    let state = list.getState();
    assert.equal(state.rows.length, 50);
    assert.equal(state.rows[0]?.id, String(dismissed));
    assert.ok(state.hasMore);
    list.loadMore();
    await list.whenIdle();
    state = list.getState();
    assert.equal(state.rows.length, 75);
    assert.equal(state.rows.at(-1)?.id, String(dismissed + 74));
    assert.ok(!state.hasMore);
  });
}

test("an exhausted dismissed source has no more rows (exhaustedDismissedSourceHasNoMoreRows)", async () => {
  const queries = new FakeQueries({ unknown: 50 });
  const list = store("unknown", queries, new FakePreferences(range(0, 50)));
  await list.refresh();
  assert.equal(list.getState().rows.length, 0);
  assert.ok(!list.getState().hasMore);
});

test("unrelated dismissals do not expand the displayed window", async () => {
  const queries = new FakeQueries({ unknown: 75 });
  const list = store("unknown", queries, new FakePreferences(range(100, 150)));
  await list.refresh();
  assert.equal(list.getState().rows.length, 50);
  assert.ok(list.getState().hasMore);
  list.loadMore();
  await list.whenIdle();
  assert.equal(list.getState().rows.length, 75);
  assert.ok(!list.getState().hasMore);
});

test("Chats counts unloaded requests up to the cap, without dismissed ones", async () => {
  const queries = new FakeQueries({ allowed: 2, unknown: 200 });
  const list = store("allowed", queries, new FakePreferences(["3", "4"]));
  await list.refresh();
  assert.equal(list.getState().requestCount, 198);
  assert.deepEqual(
    queries.lists.find((read) => read.consent === "unknown"),
    { consent: "unknown", limit: 501 },
  );
  queries.totals = { allowed: 2, unknown: 501 };
  await list.refresh();
  assert.equal(list.getState().requestCount, undefined);
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

test("Requests reads its list once, with no badge read (requestsHydrateWithoutASecondBlockedCountRead)", async () => {
  const queries = new FakeQueries({ unknown: 3 });
  const list = store("unknown", queries);
  await list.refresh();
  assert.deepEqual(queries.lists, [{ consent: "unknown", limit: 50 }]);
  assert.equal(list.getState().requestCount, undefined);
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
  const queries = new FakeQueries({ allowed: 3 });
  const list = store("allowed", queries);
  await list.refresh();
  assert.equal(list.getState().rows[0]?.title, "Group 0");
  const reads = queries.details_.length;
  let titleDuringReload: string | undefined;
  const stop = list.subscribe(() => {
    const state = list.getState();
    if (!state.loading && queries.details_.length === reads) {
      titleDuringReload = state.rows[0]?.title;
    }
  });
  await list.refresh();
  stop();
  assert.equal(titleDuringReload, "Group 0");
  assert.ok(queries.details_.length > reads);
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

test("paging stops while searching (shouldPage)", async () => {
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

test("a superseded load failure cannot change a newer load (supersededLoadFailureCannotChangeNewLoad)", async () => {
  const queries = new FakeQueries({ allowed: 2 });
  let rejectFirst: (error: Error) => void = () => {};
  let calls = 0;
  const slow: ConversationQueries = {
    list: (consent, limit) => {
      calls += 1;
      if (calls === 1) {
        return new Promise((_, reject) => {
          rejectFirst = reject;
        });
      }
      return queries.list(consent, limit);
    },
    details: (record, marker) => queries.details(record, marker),
  };
  const list = store("allowed", slow);
  const first = list.refresh();
  await list.refresh();
  rejectFirst(new Error("late"));
  await first;
  assert.equal(list.getState().error, undefined);
  assert.equal(list.getState().rows.length, 2);
});

test("results after the session ended or the store closed are dropped (staleConversationPageIsRejected)", async () => {
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

test("details read the unread count after the conversation's read marker", async () => {
  const queries = new FakeQueries({ allowed: 1 });
  const preferences = new FakePreferences();
  preferences.markers.set("0", 42n);
  const list = store("allowed", queries, preferences);
  await list.refresh();
  assert.deepEqual(queries.markers, [42n]);
});

test("a local alias names the row", async () => {
  const queries = new FakeQueries({ allowed: 1 });
  const preferences = new FakePreferences();
  preferences.aliases.set("0", "Book club");
  const list = store("allowed", queries, preferences);
  await list.refresh();
  assert.equal(list.getState().rows[0]?.title, "Book club");
  assert.equal(list.getState().rows[0]?.initials, "BC");
});

test("previews follow the iOS mapping", () => {
  const text: MessageContent = { kind: "text", value: "Hi" };
  assert.equal(previewText(text, "x"), "Hi");
  assert.equal(previewText({ kind: "markdown", value: "**b**" }, "x"), "**b**");
  const reply = (body: MessageContent): MessageContent =>
    ({ kind: "reply", referenceId: "m", body }) as MessageContent;
  assert.equal(previewText(reply({ kind: "text", value: "Yes" }), "f"), "Yes");
  assert.equal(previewText(reply({ kind: "readReceipt" }), "f"), "f");
  assert.equal(previewText(reply({ kind: "readReceipt" }), undefined), "Reply");
  assert.equal(
    previewText(
      { kind: "remoteAttachment", value: {} } as MessageContent,
      "file",
    ),
    "Attachment",
  );
  assert.equal(previewText({ kind: "readReceipt" }, "fallback"), "fallback");
  assert.equal(previewText({ kind: "readReceipt" }, undefined), "");
});

test("filterRows returns every row for an empty search", () => {
  const rows = [
    { id: "a", record: group("a"), title: "Alpha", initials: "A" },
    {
      id: "b",
      record: { ...group("b"), kind: "dm" as const, peerHandle: "nick#12" },
      title: "@nick#12",
      initials: "N",
    },
  ];
  assert.equal(filterRows(rows, ""), rows);
  assert.deepEqual(
    filterRows(rows, "NICK").map((row) => row.id),
    ["b"],
  );
});
