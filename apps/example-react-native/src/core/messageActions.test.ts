import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CONVERSATION,
  FakeConversation,
  MemoryJournal,
  OWN,
  PEER,
  reactionRecord,
  textMessage,
} from "./chatFakes.ts";
import type { ChatMessage } from "./chatMessages.ts";
import {
  MessageActionCopy,
  MessageActions,
  canDelete,
  canRetry,
} from "./messageActions.ts";
import { SendCoordinator, SendJournal } from "./sendJournal.ts";

function harness(queries = new FakeConversation()) {
  const storage = new MemoryJournal();
  const journal = new SendJournal(storage);
  const coordinator = new SendCoordinator(
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
  );
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
  // A peer's reaction with the same emoji is not the own one.
  const row: ChatMessage = {
    ...textMessage(1),
    reactions: [reactionRecord(PEER, "👍", 4)],
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
  assert.equal(h.changes(), 2);
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
  assert.equal(canDelete(own(1), OWN, false), false);
  assert.equal(canRetry({ ...own(1), delivery: "failed" }, OWN, true), true);
  assert.equal(canRetry({ ...own(1), delivery: "queued" }, OWN, true), true);
  assert.equal(canRetry(own(1), OWN, true), false);
});

test("a confirmed deletion journals its target, then publishes the operation", async () => {
  const h = harness();
  const row = own(1);
  h.queries.published.push(row);
  let journaled = false;
  const remove = h.queries.delete.bind(h.queries);
  h.queries.delete = (id) => {
    journaled = h.storage.drafts.some(
      (entry) => entry.deletionTargetId === id && entry.phase === "queueing",
    );
    return remove(id);
  };
  h.actions.requestDelete(row);
  assert.equal(h.actions.getState().deleteTarget?.id, "m0001");
  h.actions.confirmDelete();
  await h.actions.wait();
  assert.equal(journaled, true);
  assert.deepEqual(h.queries.deletions, ["m0001"]);
  assert.equal(h.queries.publications.length, 1);
  assert.deepEqual(h.storage.drafts, []);
  assert.equal(h.actions.getState().error, undefined);
  assert.equal(h.actions.getState().deleteTarget, undefined);
  assert.deepEqual((await h.queries.message("m0001"))?.content, {
    kind: "deleted",
  });
});

test("a deletion whose publication fails needs review in Settings", async () => {
  const h = harness();
  const row = own(1);
  h.queries.published.push(row);
  h.queries.publishError = new Error("offline");
  h.actions.deleteMessage(row);
  await h.actions.wait();
  assert.equal(
    h.actions.getState().error,
    MessageActionCopy.deletionNeedsReview,
  );
  assert.equal(h.storage.drafts[0]?.phase, "accepted");
  assert.equal(h.storage.drafts[0]?.deletionTargetId, "m0001");
});

test("retry publishes the same message ID", async () => {
  const h = harness();
  const row: ChatMessage = { ...own(7), id: "q7", delivery: "failed" };
  h.queries.unpublished.push(row);
  h.actions.retry(row);
  await h.actions.wait();
  assert.deepEqual(h.queries.publications, ["q7"]);
  assert.equal(h.actions.getState().error, undefined);
});

test("a failed retry shows the action error", async () => {
  const h = harness();
  const row: ChatMessage = { ...own(7), id: "q7", delivery: "failed" };
  h.queries.unpublished.push(row);
  h.queries.publishError = new Error("offline");
  h.actions.retry(row);
  await h.actions.wait();
  assert.equal(h.actions.getState().error, MessageActionCopy.failed);
});

test("load reads one message and its consent", async () => {
  const queries = new FakeConversation(2);
  queries.consent = "unknown";
  const h = harness(queries);
  h.actions.load("m0002");
  await h.actions.wait();
  assert.equal(h.actions.getState().message?.id, "m0002");
  assert.equal(h.actions.allowed, false);
  assert.equal(CONVERSATION, h.actions.getState().message?.conversationId);
});
