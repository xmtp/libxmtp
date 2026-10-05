// Check the stream workload's teardown with a fake SDK. Run: node --test
import assert from "node:assert/strict";
import { test } from "node:test";

import { measure } from "./hosts/workload.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));

// One stream sample. `fault` is "read", "publish", "end" or undefined.
async function run(t, fault) {
  const log = [];
  let clock = 100;
  t.mock.method(performance, "now", () => clock);
  // The one-second grace period before the timer is not part of this test.
  t.mock.method(globalThis, "setTimeout", (resolve) => resolve());
  let stopRead;
  const stopped = new Promise((resolve) => {
    stopRead = resolve;
  });
  const events = [{ id: "a" }, { id: "b" }];
  const stream = {
    [Symbol.asyncIterator]: () => ({
      async next() {
        if (fault === "read") throw new Error("Reader failure");
        // With a failed publisher, the read waits until the stream ends.
        if (fault === "publish") return stopped;
        return { done: false, value: events.shift() };
      },
    }),
    async end() {
      log.push("end");
      clock = 9000;
      await tick();
      stopRead({ done: true, value: undefined });
      log.push("ended");
      if (fault === "end") throw new Error("End failure");
    },
  };
  const api = {
    open: async (key) => ({ key }),
    group: async () => ({}),
    stream: async () => stream,
    async publish() {
      await tick();
      await tick();
      clock = 140;
      log.push("published");
      if (fault === "publish") throw new Error("Publisher failure");
    },
    async close(client) {
      log.push(`close ${client.key}`);
      clock = 9000;
    },
  };
  const state = {
    senderKey: "sender",
    receiverKey: "receiver",
    groupId: "group",
    ids: ["a", "b"],
    eventIds: ["a", "b"],
  };
  const outcome = await measure(api, state, "stream", "unused").then(
    (value) => ({ value }),
    (error) => ({ error }),
  );
  log.push("returned");
  return { outcome, log };
}

test("teardown is awaited once and stays outside the timer", async (t) => {
  const { outcome, log } = await run(t);
  assert.deepEqual(outcome.value, {
    duration_ms: 40,
    timing_window: { start_ms: 100, end_ms: 140 },
    streamed_events: 2,
  });
  assert.deepEqual(log, [
    "published",
    "end",
    "ended",
    "close receiver",
    "close sender",
    "returned",
  ]);
});

for (const [fault, message] of [
  ["read", "Reader failure"],
  ["publish", "Publisher failure"],
]) {
  test(`a ${fault} failure still ends the stream and closes both clients`, async (t) => {
    const { outcome, log } = await run(t, fault);
    assert.equal(outcome.error?.message, message);
    // The clients close only after the stream ended and the publisher settled.
    assert.deepEqual(log, [
      ...(fault === "read" ? [] : ["published"]),
      "end",
      "ended",
      ...(fault === "read" ? ["published"] : []),
      "close receiver",
      "close sender",
      "returned",
    ]);
  });
}

test("a failed stream end still closes both clients", async (t) => {
  const { outcome, log } = await run(t, "end");
  assert.equal(outcome.error?.message, "End failure");
  assert.deepEqual(log.slice(-3), [
    "close receiver",
    "close sender",
    "returned",
  ]);
});
