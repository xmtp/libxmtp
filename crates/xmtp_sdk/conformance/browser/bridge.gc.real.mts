import assert from "node:assert/strict";
import { Worker } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.ts";

// verifies: P56, P58
if (typeof global.gc !== "function") throw new Error("run with --expose-gc");
const worker = new Worker(new URL("./bridge.worker.mts", import.meta.url), {
  execArgv: process.execArgv,
});
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
const options: B.ClientOptions = {
  backend: B.BackendSource.Options.new({
    options: {
      url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9450",
      appVersion: undefined,
      credential: undefined,
      credentials: undefined,
    },
  }),
  storage: {
    location: B.StorageLocation.InMemory.new(),
    label: undefined,
    encryptionKey: undefined,
    pool: undefined,
    singleConnection: false,
  },
  deviceSync: false,
  registration: { auto: false, nonce: undefined },
  forkRecovery: undefined,
  workers: undefined,
};

try {
  await session.ready();
  async function createAndDrop(): Promise<void> {
    const client = await Client.create(
      session,
      {
        async identity() {
          return {
            identifier: "0x0000000000000000000000000000000000000001",
            kind: B.PublicIdentityKind.Ethereum,
          };
        },
        async kind() {
          return B.SignerKind.Eoa.new();
        },
        async sign() {
          throw new Error("registration is disabled");
        },
      },
      options,
    );
    await session.call("__bridgeGcArm", [client.handle.owner]);
  }
  await createAndDrop();
  let entered = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    global.gc();
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    const state = await session.call("__bridgeGcState", []);
    if (
      state !== null &&
      typeof state === "object" &&
      "closeEntered" in state &&
      state.closeEntered === true
    ) {
      entered = true;
      break;
    }
  }
  assert.ok(entered, "GC did not call the real WASM Client.end");
  assert.equal(
    await session.call("__bridgeGcOtherBusy", []),
    true,
    "lock released before Rust close",
  );
  await session.call("__bridgeGcAllowClose", []);
  let released = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    const state = await session.call("__bridgeGcState", []);
    if (
      state !== null &&
      typeof state === "object" &&
      "closeFinished" in state &&
      state.closeFinished === true
    ) {
      released = true;
      break;
    }
  }
  assert.ok(released, "real WASM Client.end did not complete");
  assert.equal(
    await session.call("__bridgeGcOtherBusy", []),
    false,
    "lock stayed held after Rust close",
  );
  console.log("real WASM GC waited for Client.end before pool lock release");
} finally {
  await worker.terminate();
}
