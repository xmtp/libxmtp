import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import { Backend } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";

const worker = new Worker(new URL("./bridge.worker.mts", import.meta.url), {
  execArgv: process.execArgv,
});
const endpoint: WireEndpoint = {
  postMessage(message) {
    worker.postMessage(message);
  },
  onMessage(handler) {
    worker.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    worker.on("exit", handler);
    worker.on("error", handler);
  },
};

try {
  const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  await session.ready();
  const backend = await Backend.connect(session, {
    url: "http://127.0.0.1:9450",
    appVersion: undefined,
    credentials: undefined,
  });
  assert.equal(backend.handle.type, "Backend");
  assert.equal(backend.handle.epoch, session.currentEpoch);
  backend.release();
  console.log("real WASM backend call crossed a Node worker");
} finally {
  await worker.terminate();
}
