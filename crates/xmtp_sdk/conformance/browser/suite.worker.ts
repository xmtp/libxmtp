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
  () =>
    uniffiInitAsync(
      new URL(
        "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
        import.meta.url,
      ),
    ),
  async (key, args, context) => {
    if (key === "__conformanceInner") return Promise.resolve("reentered");
    if (key === "__conformanceWait") return new Promise<never>(() => {});
    if (key === "__conformanceCrash")
      throw new WebAssembly.RuntimeError("browser conformance worker trap");
    return dispatchGenerated(key, args, context);
  },
  browserPoolLocks(),
);
