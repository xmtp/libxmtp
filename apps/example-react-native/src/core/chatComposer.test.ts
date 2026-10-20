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
  coordinatorFor,
} from "./chatFakes.ts";
import type { ComposerDraft } from "./preferences.ts";
import { type SendCoordinator, SendError, SendJournal } from "./sendJournal.ts";

class MemoryDrafts implements ComposerDraftStorage {
  saved = new Map<string, ComposerDraft>();
  failSave = false;

  load(id: string): ComposerDraft | undefined {
    return this.saved.get(id);
  }

  save(id: string, draft: ComposerDraft): void {
    if (this.failSave) throw new Error("disk full");
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
    coordinator: coordinatorFor(queries, journal),
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

test("every edit is saved, and a failed save shows its error", () => {
  const h = harness();
  h.composer.edit("he");
  h.composer.edit("hey");
  assert.equal(h.drafts.load(CONVERSATION)?.text, "hey");
  h.drafts.failSave = true;
  h.composer.edit("hey!");
  assert.equal(h.composer.getState().error, ComposerCopy.saveFailed);
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
  let reservedBeforeQueue: string | undefined;
  h.queries.onQueue = () => {
    journaledBeforeQueue = h.storage.drafts.some(
      (entry) => entry.id === "request-1" && entry.phase === "queueing",
    );
    // The draft reservation is saved before the SDK call too.
    reservedBeforeQueue = h.drafts.load(CONVERSATION)?.requestId;
  };
  h.composer.edit("hello");
  h.composer.submit();
  assert.equal(h.composer.getState().sending, true);
  assert.equal(composerEnabled(h.composer.getState()), false);
  await h.composer.wait();
  assert.equal(journaledBeforeQueue, true);
  assert.equal(reservedBeforeQueue, "request-1");
  assert.deepEqual(h.queries.queued, [{ kind: "text", text: "hello" }]);
  assert.equal(h.queries.publications.length, 1);
  assert.equal(h.queries.published.at(-1)?.id, h.queries.publications[0]);
  // Published: the journal entry is gone and the composer is empty again.
  assert.deepEqual(h.storage.drafts, []);
  const state = h.composer.getState();
  assert.equal(state.draft.text, "");
  assert.equal(composerEnabled(state), true);
  assert.equal(state.error, undefined);
  assert.ok(h.accepted > 0);
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

test("an unknown outcome locks the composer, across a reopen, until Settings clears it", async () => {
  const h = harness();
  h.queries.queueError = new Error("SDK failed after it may have queued");
  h.composer.edit("hello");
  h.composer.submit();
  await h.composer.wait();
  let state = h.composer.getState();
  assert.equal(state.unknown, true);
  assert.equal(state.error, ComposerCopy.outcomeUnknown);
  assert.equal(composerEnabled(state), false);
  // A relaunch stays locked and keeps the text.
  const relaunched = harness(h.queries, {
    drafts: h.drafts,
    storage: h.storage,
  }).composer.getState();
  assert.equal(relaunched.unknown, true);
  assert.equal(relaunched.draft.text, "hello");
  // Settings › Send needs review › Clear saved send status, then the open
  // chat reloads.
  h.journal.remove("request-1");
  const saved = h.drafts.load(CONVERSATION);
  assert.ok(saved !== undefined);
  h.drafts.save(CONVERSATION, releaseDraft(saved));
  h.composer.reload();
  state = h.composer.getState();
  assert.equal(state.unknown, false);
  assert.equal(state.error, undefined);
  assert.equal(composerEnabled(state), true);
  assert.equal(state.draft.text, "hello");
  // Clearing never resends.
  assert.deepEqual(h.queries.queued, []);
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
  assert.equal(composerEnabled(state), true);
  assert.deepEqual(h.storage.drafts, []);
});

test("text typed during a send stays after acceptance", async () => {
  const h = harness();
  h.composer.edit("first");
  let reserved: ComposerDraft | undefined;
  h.queries.onQueue = () => {
    // The UI blocks edits during a send; a later one that allows them must
    // keep the new text.
    reserved = h.drafts.load(CONVERSATION);
    if (reserved !== undefined) {
      h.drafts.save(CONVERSATION, editDraft(reserved, "first and more"));
    }
  };
  h.composer.submit();
  await h.composer.wait();
  assert.equal(reserved?.text, "first");
  assert.equal(h.drafts.load(CONVERSATION)?.text, "first and more");
  assert.equal(h.drafts.load(CONVERSATION)?.requestId, undefined);
});

test("a reservation saved before a crash resolves on reopen", () => {
  const h = harness();
  // The send was accepted while closed: the unedited text clears.
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
  let state = h.composer.getState();
  assert.equal(state.draft.text, "");
  assert.equal(state.unknown, false);
  assert.equal(composerEnabled(state), true);
  // No journal entry: the reservation is released and the text kept.
  h.storage.drafts = [];
  h.drafts.save(CONVERSATION, {
    text: "kept",
    revision: 1,
    requestId: "lost",
    submittedRevision: 1,
  });
  h.reopen();
  state = h.composer.getState();
  assert.equal(state.draft.text, "kept");
  assert.equal(composerEnabled(state), true);
});

test("closing the chat during a send stops it before the SDK and keeps the draft", async () => {
  const h = harness();
  h.composer.edit("hello");
  h.composer.submit();
  h.open = false;
  await h.composer.wait();
  assert.deepEqual(h.queries.queued, []);
  h.open = true;
  h.reopen();
  assert.equal(h.composer.getState().draft.text, "hello");
  assert.equal(composerEnabled(h.composer.getState()), true);
});
