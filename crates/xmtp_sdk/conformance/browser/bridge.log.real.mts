import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen.ts";
import { METHOD_TABLE } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/dispatch.gen.ts";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire.ts";

// verifies: P61
assert.ok("setLogSink" in METHOD_TABLE, "WASM bridge has no log sink");
assert.ok("sdkConformanceEmit" in METHOD_TABLE);
assert.ok("sdkConformanceSinkErrorCount" in METHOD_TABLE);
assert.ok("sdkConformanceSinkDroppedCount" in METHOD_TABLE);

const worker = new Worker(
  new URL("./bridge.panic.worker.mts", import.meta.url),
  {
    execArgv: process.execArgv,
  },
);
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    worker.postMessage(message, transfer);
  },
  onMessage(handler) {
    worker.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    worker.on("exit", handler);
    worker.on("error", handler);
  },
  terminate() {
    void worker.terminate();
  },
};
const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
try {
  await session.ready();
  await session.call("initLogging", [
    {
      level: 1,
      structured: false,
      performance: false,
      otel: undefined,
      resourceAttributes: new Map(),
    },
  ]);
  let delivered = 0;
  const sink = session.callbacks.register("LogSink", {
    log: () => {
      delivered++;
      return new Promise<void>(() => {});
    },
  });
  await session.call("setLogSink", [sink]);
  await session.call("sdkConformanceEmit", [4105]);
  assert.equal(await session.call("sdkConformanceSinkDroppedCount", []), 9n);
  assert.equal(await session.call("sdkConformanceSinkErrorCount", []), 0n);
  assert.equal(delivered, 1, "main thread did not acknowledge the batch");
  console.log("real WASM log sink counted nine records above its 4096 window");
} finally {
  await worker.terminate();
}
