import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type SendDraft,
  type SendJournalStorage,
  type SendQueries,
  SendCoordinator,
  SendError,
  SendJournal,
  SendReviewCopy,
  canRetryDeletion,
  composerOutcomeUnknown,
  needsReview,
  parseSendDrafts,
  reviewDetail,
  reviewTitle,
} from "./sendJournal.ts";

class MemoryStorage implements SendJournalStorage {
  drafts: readonly SendDraft[] = [];
  /** Fails the first write this matches, once. */
  failOnce: ((drafts: readonly SendDraft[]) => boolean) | undefined;

  read(): readonly SendDraft[] {
    return this.drafts;
  }

  write(drafts: readonly SendDraft[]): void {
    if (this.failOnce?.(drafts) === true) {
      this.failOnce = undefined;
      throw new SendError("storage");
    }
    this.drafts = drafts;
  }
}

/** Queued messages publish on `publish`. */
class FakeQueries implements SendQueries {
  queues = 0;
  publications = 0;
  consentValue: "allowed" | "unknown" | "denied" = "allowed";
  published = new Set<string>();
  onQueue: (() => void) | undefined;
  failPublish = false;

  queue(): Promise<string> {
    this.queues += 1;
    this.onQueue?.();
    return Promise.resolve("accepted");
  }

  consent(): Promise<"allowed" | "unknown" | "denied"> {
    return Promise.resolve(this.consentValue);
  }

  message(messageId: string) {
    return Promise.resolve({
      conversationId: "chat",
      published: this.published.has(messageId),
    });
  }

  publish(_conversationId: string, messageId: string): Promise<void> {
    this.publications += 1;
    if (this.failPublish) return Promise.reject(new Error("offline"));
    // The fake never confirms publication, so every retry publishes again.
    void messageId;
    return Promise.resolve();
  }
}

const draft = (overrides: Partial<SendDraft> = {}): SendDraft => ({
  id: "request-1",
  conversationId: "chat",
  phase: "queueing",
  createdAt: 1,
  ...overrides,
});

test("a send journals queueing before the SDK call and the accepted ID before acknowledging; Retry publishes it and clears the entry", async () => {
  const storage = new MemoryStorage();
  const queries = new FakeQueries();
  queries.failPublish = true;
  queries.onQueue = () => {
    assert.deepEqual(storage.drafts, [draft()]);
  };
  const coordinator = new SendCoordinator(
    new SendJournal(storage),
    queries,
    () => true,
  );
  let acknowledged: string | undefined;
  const id = await coordinator.queue(draft(), {
    admission: () => true,
    accept: () => queries.queue(),
    acknowledge: (messageId) => {
      assert.deepEqual(storage.drafts, [
        draft({ phase: "accepted", messageId }),
      ]);
      acknowledged = messageId;
    },
  });
  assert.equal(acknowledged, id);
  // The publication failed: the accepted entry stays for Retry, outside review.
  assert.deepEqual(storage.drafts, [
    draft({ phase: "accepted", messageId: id }),
  ]);
  assert.ok(!needsReview(storage.drafts.at(0) ?? draft()));
  queries.failPublish = false;
  await coordinator.retry("chat", id);
  assert.deepEqual(storage.drafts, []);
});

test("keeps the accepted ID in memory when its durable write fails", async () => {
  const storage = new MemoryStorage();
  storage.failOnce = (drafts) =>
    drafts.some((entry) => entry.phase === "accepted");
  const queries = new FakeQueries();
  const coordinator = new SendCoordinator(
    new SendJournal(storage),
    queries,
    () => true,
  );
  const id = await coordinator.queue(draft(), {
    admission: () => true,
    accept: () => queries.queue(),
  });
  assert.equal(id, "accepted");
  assert.equal(queries.publications, 0);
  assert.equal(storage.drafts.at(0)?.phase, "queueing");
  coordinator.recoverAccepted();
  assert.equal(storage.drafts.at(0)?.messageId, id);
  assert.equal(storage.drafts.at(0)?.phase, "accepted");
  await coordinator.retry("chat", id);
  assert.equal(queries.publications, 1);
  assert.deepEqual(storage.drafts, []);
});

test("a known rejection restores the journal; other errors leave the outcome unknown", async () => {
  for (const kind of ["rejected", "unavailable", "stale"] as const) {
    const storage = new MemoryStorage();
    const coordinator = new SendCoordinator(
      new SendJournal(storage),
      new FakeQueries(),
      () => true,
    );
    await assert.rejects(
      coordinator.queue(draft(), {
        admission: () => true,
        accept: () => Promise.reject(new SendError(kind)),
      }),
      SendError,
    );
    assert.deepEqual(storage.drafts, [], kind);
  }
  const storage = new MemoryStorage();
  const coordinator = new SendCoordinator(
    new SendJournal(storage),
    new FakeQueries(),
    () => true,
  );
  await assert.rejects(
    coordinator.queue(draft(), {
      admission: () => true,
      accept: () => Promise.reject(new Error("timeout")),
    }),
    /timeout/u,
  );
  assert.deepEqual(storage.drafts, [draft()]);
  assert.ok(composerOutcomeUnknown(storage.drafts, "chat"));
  assert.ok(!composerOutcomeUnknown(storage.drafts, "other"));
  assert.ok(needsReview(storage.drafts.at(0) ?? draft({ phase: "accepted" })));
});

test("consent, session and screen are checked before the SDK call", async () => {
  const cases = [
    {
      name: "consent unknown",
      consent: "unknown",
      session: [true],
      screen: [true],
    },
    {
      name: "consent denied",
      consent: "denied",
      session: [true],
      screen: [true],
    },
    {
      name: "session ended",
      consent: "allowed",
      session: [false],
      screen: [true],
    },
    {
      name: "screen closed",
      consent: "allowed",
      session: [true],
      screen: [false],
    },
    {
      name: "screen closed during the consent read",
      consent: "allowed",
      session: [true],
      screen: [true, false],
    },
    {
      name: "session ended during the consent read",
      consent: "allowed",
      session: [true, false],
      screen: [true],
    },
  ] as const;
  for (const { name, consent, session, screen } of cases) {
    const storage = new MemoryStorage();
    const queries = new FakeQueries();
    queries.consentValue = consent;
    // Each check reads the next value; the last one repeats.
    const reads = (values: readonly boolean[]) => {
      let index = 0;
      return () => values[Math.min(index++, values.length - 1)] ?? true;
    };
    const coordinator = new SendCoordinator(
      new SendJournal(storage),
      queries,
      reads(session),
    );
    await assert.rejects(
      coordinator.queue(draft(), {
        admission: reads(screen),
        accept: () => queries.queue(),
      }),
      (error: unknown) => error instanceof SendError && error.kind === "stale",
      name,
    );
    assert.equal(queries.queues, 0, name);
    assert.deepEqual(storage.drafts, [], name);
  }
});

test("a second submission of the same request is a duplicate", async () => {
  const storage = new MemoryStorage();
  const queries = new FakeQueries();
  const coordinator = new SendCoordinator(
    new SendJournal(storage),
    queries,
    () => true,
  );
  let release: (() => void) | undefined;
  const first = coordinator.queue(draft(), {
    admission: () => true,
    accept: () =>
      new Promise<string>((resolve) => {
        release = () => resolve("accepted");
      }),
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  await assert.rejects(
    coordinator.queue(draft(), {
      admission: () => true,
      accept: () => queries.queue(),
    }),
    (error: unknown) =>
      error instanceof SendError && error.kind === "duplicate",
  );
  release?.();
  assert.equal(await first, "accepted");
});

test("deletions are journaled and an accepted deletion is retried, not repeated", async () => {
  const storage = new MemoryStorage();
  const queries = new FakeQueries();
  queries.failPublish = true;
  const coordinator = new SendCoordinator(
    new SendJournal(storage),
    queries,
    () => true,
  );
  let deletes = 0;
  const options = {
    admission: () => true,
    deleteMessage: () => {
      deletes += 1;
      return Promise.resolve("deletion-op");
    },
    newId: () => "delete-request",
    now: () => 5,
  };
  assert.equal(
    await coordinator.delete("chat", "target", options),
    "deletion-op",
  );
  const saved = storage.drafts.at(0);
  assert.deepEqual(saved, {
    id: "delete-request",
    conversationId: "chat",
    phase: "accepted",
    messageId: "deletion-op",
    deletionTargetId: "target",
    createdAt: 5,
  });
  assert.ok(coordinator.deletionNeedsReview("target"));
  assert.equal(
    await coordinator.delete("chat", "target", options),
    "deletion-op",
  );
  assert.equal(deletes, 1);
  assert.equal(reviewTitle(saved ?? draft()), SendReviewCopy.deletionPending);
  assert.ok(canRetryDeletion(saved ?? draft()));
  // A deletion whose outcome is unknown refuses to run again.
  storage.drafts = [draft({ id: "d", deletionTargetId: "target" })];
  await assert.rejects(
    coordinator.delete("chat", "target", options),
    (error: unknown) =>
      error instanceof SendError && error.kind === "duplicate",
  );
});

test("review rows by state, and the in-chat clear never touches other conversations", () => {
  const send = draft();
  const pending = draft({
    id: "2",
    phase: "accepted",
    messageId: "m",
    deletionTargetId: "t",
  });
  const unknownDelete = draft({ id: "3", deletionTargetId: "t" });
  assert.equal(reviewTitle(send), SendReviewCopy.sendTitle);
  assert.equal(reviewDetail(send), SendReviewCopy.sendDetail);
  assert.equal(reviewTitle(unknownDelete), SendReviewCopy.deletionUnknown);
  assert.equal(reviewDetail(pending), SendReviewCopy.deletionDetail);
  assert.ok(!canRetryDeletion(unknownDelete));
  const other = draft({ id: "4", conversationId: "other" });
  assert.ok([send, pending, unknownDelete, other].every(needsReview));
  const storage = new MemoryStorage();
  storage.drafts = [send, pending, unknownDelete, other];
  new SendJournal(storage).clearUnknownSends("chat");
  assert.deepEqual(
    storage.drafts.map((entry) => entry.id),
    ["2", "3", "4"],
  );
});

test("stored journals parse strictly", () => {
  const entries = [
    draft(),
    draft({ id: "2", messageId: "m", phase: "accepted" }),
  ];
  assert.deepEqual(
    parseSendDrafts(JSON.parse(JSON.stringify(entries))),
    entries,
  );
  assert.throws(() => parseSendDrafts({}));
  assert.throws(() => parseSendDrafts([{ id: "x" }]));
  assert.throws(() => parseSendDrafts([{ ...draft(), phase: "done" }]));
  assert.throws(() => parseSendDrafts([{ ...draft(), messageId: 4 }]));
});
