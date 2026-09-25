import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { dispatchGenerated } from "../../../../target/sdk-generated/typescript-wasm/dispatch.gen";
import { uniffiInitAsync } from "../../../../target/sdk-generated/typescript-wasm/index";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import {
  browserPoolLocks,
  WorkerHost,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

let gcArmed = false;
let gcEntered = false;
let gcFinished = false;
let allowClose: (() => void) | undefined;
const closeGate = new Promise<void>((resolve) => {
  allowClose = resolve;
});

const endpoint: WireEndpoint = {
  postMessage(message, transfer) {
    self.postMessage(message, { transfer });
  },
  onMessage(handler) {
    self.addEventListener("message", (event: MessageEvent<WireMessage>) =>
      handler(event.data),
    );
  },
  onExit() {},
  close() {
    self.close();
  },
};

new WorkerHost(
  endpoint,
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  async () => {
    await uniffiInitAsync(
      new URL(
        "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
        import.meta.url,
      ),
    );
    const end = B.Client.prototype.end;
    B.Client.prototype.end = async function (...args) {
      if (gcArmed) {
        gcEntered = true;
        await closeGate;
      }
      await Reflect.apply(end, this, args);
      if (gcArmed) gcFinished = true;
    };
  },
  async (key, args, context) => {
    if (key === "__gcArm") {
      gcArmed = true;
      return undefined;
    }
    if (key === "__gcState") return { gcEntered, gcFinished };
    if (key === "__gcAllowClose") {
      allowClose?.();
      return undefined;
    }
    return dispatchGenerated(key, args, context);
  },
  browserPoolLocks(),
);
