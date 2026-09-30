import { uniffiInitAsync } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/binding";
// Loads the conformance build, whose hooks the default build must not export.
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import { dispatchGenerated } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/dispatch.gen";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";
import {
  browserPoolLocks,
  WorkerHost,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/worker/host";

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
        "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/xmtp_sdk.wasm",
        import.meta.url,
      ),
    ),
  dispatchGenerated,
  browserPoolLocks(),
);
