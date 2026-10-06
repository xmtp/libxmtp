import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen.ts";
import { METHOD_TABLE } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/dispatch.gen.ts";
import { setLogSink as setWorkerLogSink } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/proxy.gen.ts";
import { logSinkSetter } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/log-sink.ts";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire.ts";
import { loggingContract, type ContractLogRecord } from "./logging-contract.js";

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
let deliver: (message: WireMessage) => void = () => {};
let pauseNext = false;
let held: Extract<WireMessage, { t: "callback" }> | undefined;
let packetArrived: (() => void) | undefined;
const outstanding = new Set<number>();
let maximumPackets = 0;
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    if (message.t === "callbackResult") outstanding.delete(message.id);
    worker.postMessage(message, transfer);
  },
  onMessage(handler) {
    deliver = handler;
    worker.on("message", (message: WireMessage) => {
      if (message.t === "callback" && message.method === "log") {
        outstanding.add(message.id);
        maximumPackets = Math.max(maximumPackets, outstanding.size);
        if (pauseNext) {
          pauseNext = false;
          held = message;
          packetArrived?.();
          return;
        }
      }
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

function holdNextPacket(): Promise<void> {
  pauseNext = true;
  return new Promise((resolve) => {
    packetArrived = resolve;
  });
}
function releasePacket(): void {
  assert.ok(held, "no delayed log packet");
  const packet = held;
  held = undefined;
  deliver(packet);
}

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
  const api = {
    setLogSink: logSinkSetter<{
      log(record: ContractLogRecord): Promise<void>;
    }>(async (update) => update(session), setWorkerLogSink),
    async emit(count: number): Promise<void> {
      await session.call("sdkConformanceEmit", [count]);
    },
  };
  await loggingContract(api);

  // verifies: LOG-003, LOG-013
  // Delay the actual worker packet between dispatch and host handoff.
  let release!: () => void;
  let firstRecord!: (record: ContractLogRecord) => void;
  let secondRecord!: (record: ContractLogRecord) => void;
  const first = new Promise<ContractLogRecord>((resolve) => {
    firstRecord = resolve;
  });
  const second = new Promise<ContractLogRecord>((resolve) => {
    secondRecord = resolve;
  });
  const barrier = new Promise<void>((resolve) => {
    release = resolve;
  });
  let calls = 0;
  await api.setLogSink({
    async log(record) {
      calls++;
      if (calls === 1) {
        firstRecord(record);
        await barrier;
      } else if (calls === 2) {
        secondRecord(record);
        await api.setLogSink();
      }
    },
  });
  const arrived = holdNextPacket();
  await api.emit(4099);
  await arrived;
  await api.emit(2);
  releasePacket();
  assert.equal(
    (await first).droppedRecords,
    3n,
    "handoff changed the captured dispatch count",
  );
  release();
  assert.equal(
    (await second).droppedRecords,
    2n,
    "success cleared post-dispatch drops",
  );
  await api.setLogSink();

  // verifies: LOG-007, LOG-011, LOG-012
  let oldCalls = 0;
  await api.setLogSink({
    async log() {
      oldCalls++;
    },
  });
  const staleArrived = holdNextPacket();
  await api.emit(1);
  await staleArrived;
  let replacement!: () => void;
  const replaced = new Promise<void>((resolve) => {
    replacement = resolve;
  });
  await api.setLogSink({
    async log(record) {
      assert.equal(record.droppedRecords, 0n);
      replacement();
    },
  });
  await api.emit(1);
  releasePacket();
  await replaced;
  assert.equal(
    oldCalls,
    0,
    "old packet was admitted after replacement returned",
  );
  assert.equal(
    maximumPackets,
    1,
    "browser transport retained more than one dispatched log",
  );
  await api.setLogSink();
  console.log(
    "real WASM async log queue, failure, generations, reentry and delayed handoff passed",
  );
} finally {
  await worker.terminate();
}
