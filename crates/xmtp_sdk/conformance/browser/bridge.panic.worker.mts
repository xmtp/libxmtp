import { parentPort } from "node:worker_threads";

import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen.ts";
import { dispatchGenerated } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/dispatch.gen.ts";
import { uniffiInitAsync } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/index.ts";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire.ts";
import { WorkerHost } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/worker/host.ts";

if (!parentPort) throw new Error("bridge worker has no parent port");
const port = parentPort;
const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    port.postMessage(message, transfer);
  },
  onMessage(handler) {
    port.on("message", (message: WireMessage) => handler(message));
  },
  onExit(handler) {
    port.on("close", handler);
  },
  close() {
    port.close();
  },
};

const wasm = new URL(
  "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/xmtp_sdk.wasm",
  import.meta.url,
);
const host = new WorkerHost(
  endpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  async () => {
    await uniffiInitAsync(wasm);
  },
  async (key, args, context) => {
    if (key === "__bridgeNever") return new Promise<unknown>(() => {});
    if (key === "__bridgeHandle") return context.registry.add({}, "Client");
    return dispatchGenerated(key, args, context);
  },
);
process.on("unhandledRejection", (error) => host.fatal(error));
process.on("uncaughtException", (error) => host.fatal(error));
