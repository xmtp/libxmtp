import { parentPort } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen.ts";
import { dispatchGenerated } from "../../../../target/sdk-generated/typescript-wasm/dispatch.gen.ts";
import { uniffiInitAsync } from "../../../../target/sdk-generated/typescript-wasm/index.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.ts";
import { WorkerHost } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.ts";

if (!parentPort) throw new Error("bridge worker has no parent port");
const port = parentPort;
const endpoint: WireEndpoint = {
  postMessage(message) {
    port.postMessage(message);
  },
  onMessage(handler) {
    port.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    port.on("close", handler);
  },
};

const wasm = new URL(
  "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
new WorkerHost(
  endpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  async () => {
    await uniffiInitAsync(wasm);
  },
  dispatchGenerated,
);
