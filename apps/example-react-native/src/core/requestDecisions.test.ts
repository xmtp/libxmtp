import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DecisionCopy,
  type DecisionOutcome,
  RequestDecisionsStore,
} from "./requestDecisions.ts";

interface Harness {
  readonly store: RequestDecisionsStore;
  readonly writes: { id: string; consent: string }[];
  readonly dismissed: string[];
  readonly outcomes: DecisionOutcome[];
  readonly flags: { current: boolean; failDismiss: boolean };
  /** Settles the oldest pending consent write. */
  settle(error?: Error): Promise<void>;
}

function harness(): Harness {
  const pending: { resolve: () => void; reject: (error: Error) => void }[] = [];
  const writes: { id: string; consent: string }[] = [];
  const dismissed: string[] = [];
  const outcomes: DecisionOutcome[] = [];
  const flags = { current: true, failDismiss: false };
  const store = new RequestDecisionsStore({
    setConsent: (id, consent) => {
      writes.push({ id, consent });
      return new Promise((resolve, reject) => {
        pending.push({ resolve, reject });
      });
    },
    dismissRequest: (id) => {
      if (flags.failDismiss) throw new Error("disk");
      dismissed.push(id);
    },
    isCurrent: () => flags.current,
    onOutcome: (outcome) => {
      outcomes.push(outcome);
    },
  });
  return {
    store,
    writes,
    dismissed,
    outcomes,
    flags,
    async settle(error) {
      const next = pending.shift();
      if (error === undefined) next?.resolve();
      else next?.reject(error);
      await new Promise((resolve) => setImmediate(resolve));
    },
  };
}

test("accept allows the conversation at once and opens its chat", async () => {
  const h = harness();
  h.store.accept("c1");
  assert.ok(h.store.getState().busy);
  assert.deepEqual(h.writes, [{ id: "c1", consent: "allowed" }]);
  await h.settle();
  assert.ok(!h.store.getState().busy);
  assert.deepEqual(h.outcomes, [{ kind: "openChat", id: "c1" }]);
});

test("block asks first, then denies and returns to Chats", async () => {
  const h = harness();
  h.store.requestBlock("c2");
  assert.deepEqual(h.store.getState().confirmation, {
    kind: "block",
    id: "c2",
  });
  assert.equal(h.writes.length, 0);
  h.store.confirm(h.store.getState().confirmation);
  assert.equal(h.store.getState().confirmation, undefined);
  assert.deepEqual(h.writes, [{ id: "c2", consent: "denied" }]);
  await h.settle();
  assert.deepEqual(h.outcomes, [{ kind: "chats" }]);
});

test("delete is local only: it dismisses the request and returns to Chats", () => {
  const h = harness();
  h.store.requestDelete("c3");
  h.store.confirm(h.store.getState().confirmation);
  assert.deepEqual(h.dismissed, ["c3"]);
  assert.equal(h.writes.length, 0);
  assert.deepEqual(h.outcomes, [{ kind: "chats" }]);
});

test("a failed consent write shows the error; the next request clears it", async () => {
  const h = harness();
  h.store.accept("c1");
  await h.settle(new Error("network"));
  assert.equal(h.store.getState().error, DecisionCopy.updateFailed);
  assert.ok(!h.store.getState().busy);
  assert.deepEqual(h.outcomes, []);
  h.store.requestBlock("c1");
  assert.equal(h.store.getState().error, undefined);
});

test("a failed dismissal shows the delete error; a retry clears it", () => {
  const h = harness();
  h.flags.failDismiss = true;
  h.store.requestDelete("c3");
  h.store.confirm(h.store.getState().confirmation);
  assert.equal(h.store.getState().error, DecisionCopy.deleteFailed);
  assert.deepEqual(h.outcomes, []);
  h.flags.failDismiss = false;
  h.store.requestDelete("c3");
  h.store.confirm(h.store.getState().confirmation);
  assert.equal(h.store.getState().error, undefined);
  assert.deepEqual(h.outcomes, [{ kind: "chats" }]);
});

for (const end of ["open", "closed", "session ended"] as const) {
  test(`a decision confirmed while a consent write runs waits for it (${end})`, async () => {
    const h = harness();
    h.store.accept("c1");
    h.store.requestBlock("c2");
    h.store.confirm(h.store.getState().confirmation);
    assert.equal(h.writes.length, 1, "one consent write at a time");
    assert.ok(h.store.getState().busy);
    // The first decision's navigation may close the screen.
    if (end === "closed") h.store.dispose();
    if (end === "session ended") h.flags.current = false;
    await h.settle();
    await h.settle();
    assert.equal(
      h.writes.some(({ id, consent }) => id === "c2" && consent === "denied"),
      end !== "session ended",
    );
    assert.equal(
      h.outcomes.some(({ kind }) => kind === "chats"),
      end === "open",
    );
    if (end === "open") assert.ok(!h.store.getState().busy);
  });
}

test("a closed store or an ended session ignores actions and late results", async () => {
  for (const end of ["closed", "session ended"] as const) {
    const h = harness();
    h.store.accept("c1");
    h.store.requestBlock("c2");
    const shown = h.store.getState().confirmation;
    if (end === "closed") h.store.dispose();
    else h.flags.current = false;
    await h.settle();
    h.store.confirm(shown);
    h.store.requestDelete("c3");
    assert.deepEqual(h.outcomes, [], end);
    assert.deepEqual(h.writes, [{ id: "c1", consent: "allowed" }], end);
    assert.deepEqual(h.dismissed, [], end);
  }
});

test("Cancel closes the dialog, and its action still runs when the dismissal arrived first", () => {
  const h = harness();
  h.store.requestDelete("c4");
  const shown = h.store.getState().confirmation;
  h.store.cancel();
  assert.equal(h.store.getState().confirmation, undefined);
  assert.deepEqual(h.dismissed, []);
  h.store.confirm(shown);
  assert.deepEqual(h.dismissed, ["c4"]);
  assert.deepEqual(h.outcomes, [{ kind: "chats" }]);
});
