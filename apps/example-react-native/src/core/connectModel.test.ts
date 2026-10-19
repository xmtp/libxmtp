import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type ConnectJoinRequest,
  type ConnectPorts,
  type ConnectState,
  ConnectModel,
  canJoin,
} from "./connectModel.ts";
import { SessionCopy } from "./sessionError.ts";
import { deferred, settle } from "./testing.ts";

interface Harness {
  readonly model: ConnectModel;
  readonly joins: ConnectJoinRequest[];
  probes: number;
  generation: number;
  /** Answers each probe in order: an error or the `requiresToken` flag. */
  answers: (boolean | Error)[];
  /** When set, `connect` waits for this promise. */
  hold?: Promise<void>;
  joinError?: Error;
}

function harness(address = "https://example.com"): Harness {
  const box: Omit<Harness, "model"> = {
    joins: [],
    probes: 0,
    generation: 1,
    answers: [],
  };
  const ports: ConnectPorts = {
    probe: () => {
      box.probes += 1;
      const answer = box.answers.shift() ?? false;
      return answer instanceof Error
        ? Promise.reject(answer)
        : Promise.resolve({ requiresToken: answer });
    },
    connect: async (request) => {
      box.joins.push(request);
      box.generation += 1;
      request.onReserved(box.generation);
      if (box.hold !== undefined) await box.hold;
      if (box.joinError !== undefined) throw box.joinError;
    },
    generation: () => box.generation,
    sleep: (_ms, signal) =>
      signal.aborted ? Promise.reject(new Error("aborted")) : Promise.resolve(),
  };
  const model = new ConnectModel(address, ports);
  return Object.assign(box, { model }) as Harness;
}

const base: ConnectState = {
  address: "https://example.com",
  token: "",
  serverName: "",
  checking: false,
  ready: false,
  joining: false,
  requiresToken: false,
};

test("Join needs a valid address, no running work and a token when required", () => {
  assert.equal(canJoin(base), true);
  assert.equal(
    canJoin({ ...base, error: SessionCopy.unreachable }),
    true,
    "a failed probe can be retried with Join",
  );
  assert.equal(canJoin({ ...base, address: "  " }), false);
  assert.equal(canJoin({ ...base, address: "not a url" }), false);
  assert.equal(canJoin({ ...base, address: "http://example.com" }), false);
  assert.equal(canJoin({ ...base, checking: true }), false);
  assert.equal(canJoin({ ...base, joining: true }), false);
  assert.equal(canJoin({ ...base, requiresToken: true }), false);
  assert.equal(canJoin({ ...base, requiresToken: true, token: " \n" }), false);
  assert.equal(canJoin({ ...base, requiresToken: true, token: "t" }), true);
});

test("the background probe is quiet until Join, and Retry reveals a required token", async () => {
  const h = harness();
  h.answers = [new Error("down"), new Error("down")];
  h.model.appear();
  assert.equal(h.model.getState().checking, true);
  await settle();
  let state = h.model.getState();
  assert.equal(state.error, undefined, "quiet before Join");
  assert.equal(state.ready, false);
  assert.equal(canJoin(state), true);

  h.model.join();
  await settle();
  state = h.model.getState();
  assert.equal(state.error, SessionCopy.unreachable);
  assert.equal(state.joining, false);
  assert.equal(h.joins.length, 0);

  h.answers = [true];
  h.model.retryProbe();
  assert.equal(h.model.getState().checking, true);
  assert.equal(canJoin(h.model.getState()), false);
  await settle();
  state = h.model.getState();
  assert.equal(state.error, undefined);
  assert.equal(state.ready, true);
  assert.equal(state.requiresToken, true);
  assert.equal(canJoin(state), false);
  h.model.setToken("fixture-token");
  assert.equal(canJoin(h.model.getState()), true);

  // An address change clears the token and its requirement, and its probe is
  // quiet again although Join was pressed for the old address.
  h.answers = [new Error("down")];
  h.model.setAddress("https://other.example.com");
  state = h.model.getState();
  assert.equal(state.token, "");
  assert.equal(state.requiresToken, false);
  await settle();
  assert.equal(h.model.getState().error, undefined, "quiet after an edit");
});

test("Join probes first and joins only when no token is required", async () => {
  for (const requiresToken of [false, true]) {
    const h = harness();
    h.answers = [new Error("down"), requiresToken];
    h.model.setServerName("Staging");
    h.model.appear();
    await settle();
    h.model.join();
    // The same address text does not cancel the queued join.
    h.model.setAddress("https://example.com");
    await settle();
    const joins = h.joins.map(({ url, name }) => ({ url, name }));
    assert.deepEqual(
      joins,
      requiresToken ? [] : [{ url: "https://example.com", name: "Staging" }],
    );
    assert.equal(h.model.getState().requiresToken, requiresToken);
    assert.equal(h.model.getState().joining, false);
    assert.equal(h.model.getState().error, undefined);
  }
});

test("a successful join clears the token from UI state", async () => {
  const h = harness();
  h.answers = [true];
  h.model.appear();
  await settle();
  h.model.setToken("fixture-token");
  h.model.join();
  await settle();
  assert.equal(h.joins[0]?.token, "fixture-token");
  assert.equal(h.model.getState().token, "");
  assert.equal(h.model.getState().joining, false);
});

test("a join failure shows its message", async () => {
  const h = harness();
  h.model.appear();
  await settle();
  h.joinError = new Error(SessionCopy.tokenRejected);
  h.model.join();
  assert.equal(h.model.getState().joining, true);
  await settle();
  assert.equal(h.model.getState().error, SessionCopy.tokenRejected);
  assert.equal(h.model.getState().joining, false);
});

test("a changed server clears the superseded join and drops its late result", async () => {
  const h = harness("http://localhost:5050");
  h.model.appear();
  await settle();
  const hold = deferred();
  h.hold = hold.promise;
  h.joinError = new Error("late failure");
  h.model.join();
  await settle();
  const first = h.joins[0];
  assert.ok(first);
  h.model.setAddress("http://localhost:5051");
  assert.equal(h.model.getState().joining, false);
  assert.equal(first.signal.aborted, true, "the old join is cancelled");
  await settle();
  assert.equal(h.model.getState().ready, true);
  hold.resolve();
  await settle();
  assert.equal(h.model.getState().error, undefined, "late result dropped");
  assert.equal(canJoin(h.model.getState()), true);
});

test("results are dropped after the screen closes or the session generation changes; appear reopens", async () => {
  const h = harness();
  h.model.appear();
  h.model.dispose();
  await settle();
  assert.equal(h.model.getState().ready, false);
  // A React effect runs again after its cleanup.
  h.model.appear();
  await settle();
  assert.equal(h.model.getState().ready, true);
  h.model.setAddress("https://other.example.com");
  assert.equal(h.model.getState().address, "https://other.example.com");

  const other = harness();
  other.model.appear();
  other.generation += 1;
  await settle();
  assert.equal(other.model.getState().ready, false);
});

test("an invalid address does not probe", async () => {
  const h = harness("not a url");
  h.model.appear();
  await settle();
  assert.equal(h.probes, 0);
  assert.equal(h.model.getState().checking, false);
});

test("Connect appearing again keeps the address and server name and clears the token", async () => {
  const h = harness();
  h.answers = [true, true];
  h.model.appear();
  h.model.setAddress("https://other.example.com");
  h.model.setServerName("Local box");
  await settle();
  h.model.setToken("fixture-token");
  // Sign out or Delete local data: the screen closes and opens again.
  h.model.dispose();
  h.model.appear();
  const state = h.model.getState();
  assert.equal(state.address, "https://other.example.com");
  assert.equal(state.serverName, "Local box");
  assert.equal(state.token, "");
  assert.equal(state.joining, false);
});
