// Check the stream workload's teardown with a fake SDK, and the browser
// long-task window. Run: node --test
import assert from "node:assert/strict";
import { test } from "node:test";

import { publicApi } from "./hosts/sdk.mjs";
import { longTasksInWindow, measure } from "./hosts/workload.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));

test("stream adapter uses the supplied group and waits for its reader", async () => {
  let release;
  let calls = 0;
  const ready = new Promise((resolve) => {
    release = resolve;
  });
  const stream = { ready: () => ready };
  const group = {
    streamMessages() {
      calls += 1;
      return stream;
    },
  };
  const api = publicApi({}, {}, "node", "unused", {});
  let settled = false;
  const opened = api.stream({}, group).then((value) => {
    settled = true;
    return value;
  });
  void opened.catch(() => {});
  await tick();
  assert.equal(calls, 1);
  assert.equal(settled, false);
  release();
  assert.equal(await opened, stream);
});

// One stream sample. `fault` is "read", "duplicate", "publish", "end" or
// undefined.
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
  // A duplicate stream delivers "a" twice before "b".
  const events =
    fault === "duplicate"
      ? [{ id: "a" }, { id: "a" }, { id: "b" }]
      : [{ id: "a" }, { id: "b" }];
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

// A duplicate expected event is a read failure.
for (const [fault, message] of [
  ["read", "Reader failure"],
  ["duplicate", "Duplicate expected stream event"],
  ["publish", "Publisher failure"],
]) {
  test(`a ${fault} failure still ends the stream and closes both clients`, async (t) => {
    const { outcome, log } = await run(t, fault);
    assert.equal(outcome.error?.message, message);
    // The clients close only after the stream ended and the publisher settled.
    const readFailure = fault !== "publish";
    assert.deepEqual(log, [
      ...(readFailure ? [] : ["published"]),
      "end",
      "ended",
      ...(readFailure ? ["published"] : []),
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

test("a long task counts with its time inside the timed window", () => {
  const window = { start_ms: 1000, end_ms: 2000 };
  const entries = [
    // 60 ms task, 40 ms of it before the end of the window.
    { startTime: 1960, duration: 60 },
    // 60 ms task, 20 ms of it after the start of the window.
    { startTime: 960, duration: 60 },
    // Inside the window.
    { startTime: 1500, duration: 70 },
    // Before and after the window.
    { startTime: 900, duration: 80 },
    { startTime: 2000, duration: 55 },
  ];
  assert.deepEqual(longTasksInWindow(entries, window), [40, 20, 70]);
});
