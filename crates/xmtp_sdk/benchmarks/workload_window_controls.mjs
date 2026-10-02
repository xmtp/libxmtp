import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { SourceTextModule, createContext } from "node:vm";

const source = await readFile(
  new URL("./hosts/workload.mjs", import.meta.url),
  "utf8",
);
const liveSource = await readFile(
  new URL("./hosts/live.mjs", import.meta.url),
  "utf8",
);
for (const cleanupTime of [900, 9000]) {
  let clock = 100;
  const samples = [];
  let release;
  let entered;
  let ended = false;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const atEnd = new Promise((resolve) => {
    entered = resolve;
  });
  const closed = [];
  const stream = {
    async *[Symbol.asyncIterator]() {
      yield { id: "message", kind: "text", text: "body" };
    },
    async end() {
      clock = cleanupTime;
      entered();
      await held;
      ended = true;
    },
  };
  const context = createContext({
    performance: {
      now: () => {
        samples.push(clock);
        return clock;
      },
    },
    setTimeout: (resolve) => resolve(),
  });
  const module = new SourceTextModule(source, { context });
  await module.link(async (specifier) => {
    assert.equal(specifier, "./live.mjs");
    const live = new SourceTextModule(liveSource, { context });
    await live.link(() => {
      throw new Error("Unexpected live import");
    });
    return live;
  });
  await module.evaluate();
  const api = {
    open: async (key) => ({ key }),
    group: async () => ({}),
    stream: async () => stream,
    publish: async () => {
      clock = 140;
    },
    live: (message) => message,
    close: async (client) => {
      closed.push(client.key);
      clock = 99999;
    },
  };
  let settled = false;
  const pending = module.namespace
    .measure(
      api,
      {},
      {
        senderKey: "sender",
        receiverKey: "receiver",
        ids: ["message"],
        eventIds: ["message"],
      },
      "stream",
    )
    .then((value) => {
      settled = true;
      return value;
    });
  await atEnd;
  await new Promise((resolve) => setImmediate(resolve));
  try {
    assert.equal(settled, false, "Cleanup must be awaited before returning");
    assert.deepEqual(
      samples,
      [100, 140],
      "The measured window must end before stream cleanup",
    );
  } finally {
    release();
  }
  const result = await pending;
  assert.equal(
    result.duration_ms,
    40,
    "Cleanup time must not change stream duration",
  );
  assert.deepEqual({ ...result.timing_window }, { start_ms: 100, end_ms: 140 });
  assert.equal(result.observed_messages[0].text, "body");
  assert.equal(ended, true);
  assert.deepEqual(closed, ["receiver", "sender"]);
}
console.log("Stream operation window and awaited cleanup controls passed");
