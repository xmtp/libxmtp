import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { SourceTextModule, SyntheticModule, createContext } from "node:vm";

const source = await readFile(
  new URL("./hosts/workload.mjs", import.meta.url),
  "utf8",
);
const liveSource = await readFile(
  new URL("./hosts/live.mjs", import.meta.url),
  "utf8",
);
const readerSource = await readFile(
  new URL(
    "../../../apps/xmtp_sdk_bindgen/runtime/ts/streams/reader.ts",
    import.meta.url,
  ),
  "utf8",
);
for (const [mode, cleanupTime, expectedError] of [
  ["success", 900, null],
  ["success", 9000, null],
  ["missing", 900, "Stream ended with missing messages"],
  ["reader_error", 900, "Reader failure"],
  ["bad_body", 900, "Missing live text or reply body"],
  ["publish_error", 900, "Publisher failure"],
]) {
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
  let endCount = 0;
  const reader = {
    next: async () => {
      if (mode === "missing") return undefined;
      if (mode === "reader_error") throw new Error("Reader failure");
      return {
        id: "message",
        kind: "text",
        text: mode === "bad_body" ? undefined : "body",
      };
    },
    async end() {
      endCount += 1;
      clock = cleanupTime;
      entered();
      await held;
      ended = true;
    },
  };
  const context = createContext({
    AbortController,
    console,
    performance: {
      now: () => {
        samples.push(clock);
        return clock;
      },
    },
    setTimeout: (resolve) => resolve(),
  });
  const readerModule = new SourceTextModule(
    stripTypeScriptTypes(readerSource),
    { context },
  );
  await readerModule.link((specifier) => {
    assert.equal(specifier, "../../xmtp_sdk");
    return new SyntheticModule(
      ["ConnectionState"],
      function () {
        this.setExport("ConnectionState", { Closed: "closed" });
      },
      { context },
    );
  });
  await readerModule.evaluate();
  const stream = new readerModule.namespace.ReaderStream(
    async () => reader,
    {},
  );
  await stream.ready();
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
      if (mode === "publish_error") throw new Error("Publisher failure");
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
  const beforeRelease = { samples: [...samples], settled, ended };
  release();
  const result = await pending.catch((error) => ({ error }));
  console.log(
    JSON.stringify({
      mode,
      cleanupTime,
      beforeRelease,
      duration_ms: result.duration_ms,
      endCount,
    }),
  );
  assert.equal(
    beforeRelease.settled,
    false,
    "Cleanup must be awaited before returning",
  );
  if (expectedError !== null) {
    assert.equal(
      result.error?.message,
      expectedError,
      "The original failure must propagate",
    );
  } else {
    assert.deepEqual(
      beforeRelease.samples,
      [100, 140],
      "The measured window must end before stream cleanup",
    );
    assert.equal(
      result.duration_ms,
      40,
      "Cleanup time must not change stream duration",
    );
    assert.deepEqual(
      { ...result.timing_window },
      { start_ms: 100, end_ms: 140 },
    );
    assert.equal(result.observed_messages[0].text, "body");
  }
  assert.equal(ended, true);
  assert.equal(endCount, 1, "Actual reader cleanup must complete once");
  assert.deepEqual(closed, ["receiver", "sender"]);
}
console.log("Stream operation window and awaited cleanup controls passed");
