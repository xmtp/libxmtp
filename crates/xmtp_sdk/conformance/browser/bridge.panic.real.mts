import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen.ts";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  HandleWire,
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire.ts";

const worker = new Worker(
  new URL("./bridge.panic.worker.mts", import.meta.url),
  {
    execArgv: process.execArgv,
  },
);
let sawFatal = false;
let exits = 0;
const exited = new Promise<void>((resolve) => {
  worker.once("exit", () => {
    exits++;
    resolve();
  });
});
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    worker.postMessage(message, transfer);
  },
  onMessage(handler) {
    worker.on("message", (message: WireMessage) => {
      if (message.t === "fatal") sawFatal = true;
      handler(message);
    });
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
  const value = await session.call("__bridgeHandle", []);
  assert.ok(value !== null && typeof value === "object");
  assert.ok("h" in value && typeof value.h === "number");
  assert.ok("owner" in value && typeof value.owner === "number");
  assert.ok("epoch" in value && typeof value.epoch === "number");
  assert.ok("type" in value && value.type === "Client");
  const handle: HandleWire = {
    h: value.h,
    owner: value.owner,
    epoch: value.epoch,
    type: value.type,
  };
  const pending = session.call("__bridgeNever", []);
  const trap = session.call("bridgeTestPanic", []);
  await assert.rejects(trap, { code: "workerTerminated" });
  await assert.rejects(pending, { code: "workerTerminated" });
  assert.ok(sawFatal, "real WASM trap must send fatal");
  assert.throws(() => session.checkHandle(handle), { code: "clientClosed" });
  await Promise.race([
    exited,
    new Promise<never>((_resolve, reject) =>
      setTimeout(() => reject(new Error("poisoned worker did not exit")), 5000),
    ),
  ]);
  assert.equal(exits, 1);
  console.log("test-only real WASM trap sent fatal and stopped the worker");
} finally {
  await worker.terminate();
}

const backgroundWorker = new Worker(
  new URL("./bridge.panic.worker.mts", import.meta.url),
  { execArgv: process.execArgv },
);
let backgroundFatal = false;
const backgroundEndpoint: WireEndpoint = {
  postMessage(message, transfer) {
    backgroundWorker.postMessage(message, transfer);
  },
  onMessage(handler) {
    backgroundWorker.on("message", (message: WireMessage) => {
      if (message.t === "fatal") backgroundFatal = true;
      handler(message);
    });
  },
  onExit(handler) {
    backgroundWorker.on("exit", handler);
    backgroundWorker.on("error", handler);
  },
  terminate() {
    void backgroundWorker.terminate();
  },
};
const backgroundSession = new MainSession(
  backgroundEndpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
);
try {
  await backgroundSession.ready();
  const waiting = backgroundSession.call("__bridgeNever", []);
  const trigger = backgroundSession.call("bridgeTestBackgroundPanic", []);
  await Promise.allSettled([trigger, waiting]);
  await assert.rejects(waiting, { code: "workerTerminated" });
  assert.ok(backgroundFatal, "background WASM panic must send fatal at once");
  console.log("background WASM panic hook sent fatal without a later call");
} finally {
  await backgroundWorker.terminate();
}
