import assert from "node:assert/strict";
import { test } from "node:test";

import {
  ChatComposer,
  type ComposerDraftStorage,
  ComposerCopy,
  composerEnabled,
  editDraft,
  releaseDraft,
} from "./chatComposer.ts";
import {
  CONVERSATION,
  FakeConversation,
  MemoryJournal,
  settle,
} from "./chatFakes.ts";
import type { ComposerDraft } from "./preferences.ts";
import { SendCoordinator, SendError, SendJournal } from "./sendJournal.ts";

class MemoryDrafts implements ComposerDraftStorage {
  saved = new Map<string, ComposerDraft>();
  saves = 0;
  failSave = false;

  load(id: string): ComposerDraft | undefined {
    return this.saved.get(id);
  }

  save(id: string, draft: ComposerDraft): void {
    if (this.failSave) throw new Error("disk full");
    this.saves += 1;
    this.saved.set(id, draft);
  }
}

interface Harness {
  readonly queries: FakeConversation;
  readonly drafts: MemoryDrafts;
  readonly storage: MemoryJournal;
  readonly journal: SendJournal;
  readonly coordinator: SendCoordinator;
  accepted: number;
  open: boolean;
  composer: ChatComposer;
  reopen(): ChatComposer;
}

function harness(
  queries = new FakeConversation(1),
  shared?: { drafts: MemoryDrafts; storage: MemoryJournal },
): Harness {
  const drafts = shared?.drafts ?? new MemoryDrafts();
  const storage = shared?.storage ?? new MemoryJournal();
  const journal = new SendJournal(storage);
  let next = 0;
  const h = {
    queries,
    drafts,
    storage,
    journal,
    coordinator: new SendCoordinator(
      journal,
      {
        consent: async (id) => (await queries.conversation(id)).consent,
        message: async (id) => {
          const row = await queries.message(id);
          return row === undefined
            ? undefined
            : {
                conversationId: row.conversationId,
                published: row.delivery === "published",
              };
        },
        publish: (conversationId, messageId) =>
          queries.publish(conversationId, messageId),
      },
      () => true,
    ),
    accepted: 0,
    open: true,
    composer: undefined as unknown as ChatComposer,
    reopen: () => h.composer,
  };
  h.reopen = () => {
    h.composer = new ChatComposer({
      conversationId: CONVERSATION,
      drafts,
      journal,
      coordinator: h.coordinator,
      queries,
      admit: () => h.open,
      accepted: () => {
        h.accepted += 1;
      },
      newId: () => {
        next += 1;
        return `request-${String(next)}`;
      },
      now: () => 1_000,
    });
    h.composer.allow(true);
    return h.composer;
  };
  h.reopen();
  return h;
}

test("every edit is saved, and the revision counts changes only", () => {
  const h = harness();
  h.composer.edit("he");
  h.composer.edit("hey");
  h.composer.edit("hey");
  assert.deepEqual(h.drafts.load(CONVERSATION), { text: "hey", revision: 2 });
  assert.equal(h.drafts.saves, 2);
  assert.deepEqual(editDraft({ text: "a", revision: 4 }, "a"), {
    text: "a",
    revision: 4,
  });
});

test("the composer is disabled until consent is allowed", () => {
  const h = harness();
  h.composer.allow(false);
  h.composer.edit("blocked");
  assert.equal(h.composer.getState().draft.text, "");
  assert.equal(composerEnabled(h.composer.getState()), false);
});

test("a send journals before queueing, clears the draft, then publishes the same ID", async () => {
  const h = harness();
  let journaledBeforeQueue = false;
  h.queries.onQueue = () => {
    journaledBeforeQueue = h.storage.drafts.some(
      (entry) => entry.id === "request-1" && entry.phase === "queueing",
    );
    // The draft reservation is saved before the SDK call too.
    assert.equal(h.drafts.load(CONVERSATION)?.requestId, "request-1");
  };
  h.composer.edit("hello");
  h.composer.submit();
  assert.equal(h.composer.getState().sending, true);
  assert.equal(composerEnabled(h.composer.getState()), false);
  await h.composer.wait();
  assert.equal(journaledBeforeQueue, true);
  assert.deepEqual(h.queries.queued, [{ kind: "text", text: "hello" }]);
  assert.equal(h.queries.publications.length, 1);
  assert.equal(h.queries.published.at(-1)?.id, h.queries.publications[0]);
  // Published: the journal entry is gone and the composer is empty again.
  assert.deepEqual(h.storage.drafts, []);
  const state = h.composer.getState();
  assert.equal(state.draft.text, "");
  assert.equal(state.draft.requestId, undefined);
  assert.equal(state.sending, false);
  assert.equal(state.error, undefined);
  assert.equal(h.accepted, 1);
});

test("whitespace-only text does not send", async () => {
  const h = harness();
  h.composer.edit("   ");
  h.composer.submit();
  await h.composer.wait();
  assert.deepEqual(h.queries.queued, []);
});

test("a reply sends the reply and clears the reply target", async () => {
  const h = harness();
  h.composer.reply("m0001");
  h.composer.edit("sure");
  h.composer.submit();
  await h.composer.wait();
  assert.deepEqual(h.queries.queued, [
    { kind: "reply", parentId: "m0001", text: "sure" },
  ]);
  assert.equal(h.composer.getState().draft.replyId, undefined);
});

test("a publication failure keeps the accepted message for Retry", async () => {
  const h = harness();
  h.queries.publishError = new Error("offline");
  h.composer.edit("hello");
  h.composer.submit();
  await h.composer.wait();
  const [entry] = h.storage.drafts;
  assert.equal(entry?.phase, "accepted");
  assert.equal(h.composer.getState().draft.text, "");
  assert.equal(h.composer.getState().unknown, false);
  // Retry publishes the same ID.
  h.queries.publishError = undefined;
  assert.ok(entry?.messageId !== undefined);
  await h.coordinator.retry(CONVERSATION, entry.messageId);
  assert.deepEqual(h.queries.publications, [entry.messageId, entry.messageId]);
  assert.deepEqual(h.storage.drafts, []);
});

test("an unknown outcome locks the composer until review clears it", async () => {
  const h = harness();
  h.queries.queueError = new Error("SDK failed after it may have queued");
  h.composer.edit("hello");
  h.composer.submit();
  await h.composer.wait();
  let state = h.composer.getState();
  assert.equal(state.unknown, true);
  assert.equal(state.error, ComposerCopy.outcomeUnknown);
  assert.equal(composerEnabled(state), false);
  assert.equal(h.storage.drafts[0]?.phase, "queueing");
  assert.equal(h.storage.drafts[0]?.messageId, undefined);
  // A relaunch stays locked.
  h.reopen();
  assert.equal(h.composer.getState().unknown, true);
  assert.equal(h.composer.getState().draft.text, "hello");
  // Clearing removes the entry and never resends.
  h.composer.allow(true);
  h.composer.clearReview();
  state = h.composer.getState();
  assert.equal(state.unknown, false);
  assert.equal(composerEnabled(state), true);
  assert.deepEqual(h.storage.drafts, []);
  assert.deepEqual(h.queries.queued, []);
  assert.equal(state.draft.text, "hello");
});

test("a known rejection keeps the draft and restores the journal", async () => {
  const h = harness();
  h.queries.queueError = new SendError("unavailable");
  h.composer.edit("hello");
  h.composer.submit();
  await h.composer.wait();
  const state = h.composer.getState();
  assert.equal(state.unknown, false);
  assert.equal(state.error, ComposerCopy.rejected);
  assert.equal(state.draft.text, "hello");
  assert.equal(state.draft.requestId, undefined);
  assert.equal(composerEnabled(state), true);
  assert.deepEqual(h.storage.drafts, []);
});

test("text typed during a send stays after acceptance", async () => {
  const h = harness();
  h.composer.edit("first");
  let edited = false;
  h.queries.onQueue = () => {
    // The reservation blocks edits in the UI; a reopened composer could edit.
    const saved = h.drafts.load(CONVERSATION);
    assert.ok(saved !== undefined);
    h.drafts.save(CONVERSATION, editDraft(saved, "first and more"));
    edited = true;
  };
  h.composer.submit();
  await h.composer.wait();
  assert.equal(edited, true);
  assert.equal(h.drafts.load(CONVERSATION)?.text, "first and more");
  assert.equal(h.drafts.load(CONVERSATION)?.requestId, undefined);
});

test("an acceptance that finished while closed applies on reopen", async () => {
  const shared = { drafts: new MemoryDrafts(), storage: new MemoryJournal() };
  const h = harness(new FakeConversation(1), shared);
  h.drafts.save(CONVERSATION, {
    text: "sent before the crash",
    revision: 3,
    requestId: "request-9",
    submittedRevision: 3,
  });
  h.storage.drafts = [
    {
      id: "request-9",
      conversationId: CONVERSATION,
      phase: "accepted",
      messageId: "q1",
      createdAt: 1,
    },
  ];
  h.reopen();
  const state = h.composer.getState();
  assert.equal(state.draft.text, "");
  assert.equal(state.draft.requestId, undefined);
  assert.equal(state.unknown, false);
});

test("a reservation without a journal entry is released on reopen", () => {
  const h = harness();
  h.drafts.save(CONVERSATION, {
    text: "kept",
    revision: 1,
    requestId: "lost",
    submittedRevision: 1,
  });
  h.reopen();
  assert.equal(h.composer.getState().draft.requestId, undefined);
  assert.equal(h.composer.getState().draft.text, "kept");
});

test("a failed draft save shows its error", () => {
  const h = harness();
  h.drafts.failSave = true;
  h.composer.edit("x");
  assert.equal(h.composer.getState().error, ComposerCopy.saveFailed);
});

test("a closed composer ignores the send result", async () => {
  const h = harness();
  h.composer.edit("hello");
  h.composer.submit();
  h.open = false;
  await h.composer.wait();
  await settle();
  assert.equal(h.composer.getState().sending, true);
});

test("Settings clearing the outcome unlocks the open composer on reload", async () => {
  const h = harness();
  h.queries.queueError = new Error("SDK failed after it may have queued");
  h.composer.edit("hello");
  h.composer.submit();
  await h.composer.wait();
  assert.equal(h.composer.getState().error, ComposerCopy.outcomeUnknown);
  // Settings › Send needs review › Clear saved send status.
  h.journal.remove("request-1");
  const saved = h.drafts.load(CONVERSATION);
  assert.ok(saved !== undefined);
  h.drafts.save(CONVERSATION, releaseDraft(saved));
  h.composer.reload();
  const state = h.composer.getState();
  assert.equal(state.unknown, false);
  assert.equal(state.error, undefined);
  assert.equal(composerEnabled(state), true);
  assert.equal(state.draft.text, "hello");
});
