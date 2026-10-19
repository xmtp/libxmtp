import assert from "node:assert/strict";
import { test } from "node:test";

import {
  FakeConversation,
  MemoryJournal,
  OWN,
  PEER,
  coordinatorFor,
  reactionRecord,
  textMessage,
} from "./chatFakes.ts";
import type { ChatMessage } from "./chatMessages.ts";
import {
  MessageActionCopy,
  MessageActions,
  canDelete,
} from "./messageActions.ts";
import { SendJournal } from "./sendJournal.ts";

function harness(queries = new FakeConversation()) {
  const storage = new MemoryJournal();
  const coordinator = coordinatorFor(queries, new SendJournal(storage));
  let changes = 0;
  let next = 0;
  const actions = new MessageActions({
    queries,
    coordinator,
    own: OWN,
    admit: () => true,
    changed: () => {
      changes += 1;
    },
    newId: () => {
      next += 1;
      return `entry-${String(next)}`;
    },
    now: () => 1,
  });
  actions.allowed = true;
  return { queries, storage, actions, changes: () => changes };
}

const own = (n: number): ChatMessage => textMessage(n, { sender: OWN });

test("reacting adds the emoji, and reacting again removes it", async () => {
  const h = harness();
  // A peer's 👍 and the own ❤️ are not an own 👍.
  const row: ChatMessage = {
    ...textMessage(1),
    reactions: [reactionRecord(PEER, "👍", 4), reactionRecord(OWN, "❤️", 3)],
  };
  h.queries.published.push(row);
  h.actions.react(row, "👍");
  await h.actions.wait();
  const mine: ChatMessage = {
    ...row,
    reactions: [...row.reactions, reactionRecord(OWN, "👍", 5)],
  };
  h.actions.react(mine, "👍");
  await h.actions.wait();
  assert.deepEqual(h.queries.queued, [
    { kind: "reaction", parentId: "m0001", emoji: "👍", remove: false },
    { kind: "reaction", parentId: "m0001", emoji: "👍", remove: true },
  ]);
  assert.ok(h.changes() > 0);
});

test("actions other than copy need an allowed conversation", async () => {
  const h = harness();
  h.actions.allowed = false;
  const row = own(1);
  h.queries.published.push(row);
  h.actions.react(row, "👍");
  h.actions.deleteMessage(row);
  h.actions.requestDelete(row);
  await h.actions.wait();
  assert.deepEqual(h.queries.queued, []);
  assert.deepEqual(h.queries.deletions, []);
  assert.equal(h.actions.getState().deleteTarget, undefined);
});

test("delete rules: own, published, not deleted", () => {
  assert.equal(canDelete(own(1), OWN, true), true);
  assert.equal(canDelete(textMessage(1, { sender: PEER }), OWN, true), false);
  assert.equal(canDelete({ ...own(1), delivery: "queued" }, OWN, true), false);
  assert.equal(
    canDelete({ ...own(1), content: { kind: "deleted" } }, OWN, true),
    false,
  );
});

test("a deletion journals its target, needs review until it publishes, and is never repeated", async (t) => {
  t.mock.method(console, "warn", () => {});
  const h = harness();
  const row = own(1);
  h.queries.published.push(row);
  let journaled = false;
  h.queries.onDelete = (id) => {
    journaled = h.storage.drafts.some(
      (entry) => entry.deletionTargetId === id && entry.phase === "queueing",
    );
  };
  // The "Delete message?" alert deletes the row it showed, then dismisses.
  h.actions.requestDelete(row);
  const target = h.actions.getState().deleteTarget;
  assert.equal(target?.id, "m0001");
  h.queries.publishError = new Error("offline");
  h.actions.deleteMessage(target ?? row);
  h.actions.cancelDelete();
  await h.actions.wait();
  assert.equal(journaled, true);
  assert.equal(
    h.actions.getState().error,
    MessageActionCopy.deletionNeedsReview,
  );
  assert.equal(h.actions.getState().deleteTarget, undefined);
  // Deleting again publishes the accepted deletion instead of a new one.
  h.queries.publishError = undefined;
  h.actions.deleteMessage(row);
  await h.actions.wait();
  assert.deepEqual(h.queries.deletions, ["m0001"]);
  assert.deepEqual(h.storage.drafts, []);
  assert.equal(h.actions.getState().error, undefined);
  assert.deepEqual((await h.queries.message("m0001"))?.content, {
    kind: "deleted",
  });
});

test("retry publishes the same message ID, and a failure shows the action error", async (t) => {
  t.mock.method(console, "warn", () => {});
  const h = harness();
  const row: ChatMessage = { ...own(7), id: "q7", delivery: "failed" };
  h.queries.unpublished.push(row);
  h.queries.publishError = new Error("offline");
  h.actions.retry(row);
  await h.actions.wait();
  assert.equal(h.actions.getState().error, MessageActionCopy.failed);
  h.queries.publishError = undefined;
  h.actions.retry(row);
  await h.actions.wait();
  assert.deepEqual(h.queries.publications, ["q7", "q7"]);
  assert.equal(h.actions.getState().error, undefined);
});

test("load reads one message and its consent", async () => {
  const queries = new FakeConversation(2);
  queries.consent = "unknown";
  const h = harness(queries);
  h.actions.load("m0002");
  await h.actions.wait();
  assert.equal(h.actions.getState().message?.id, "m0002");
  assert.equal(h.actions.allowed, false);
});
