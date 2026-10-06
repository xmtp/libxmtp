// A plain generated worker host, without fixture hooks. Proofs that need
// their own session to the shipped worker build start this file.
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { dispatchGenerated } from "../../../../target/sdk-generated/typescript-wasm/dispatch.gen";
import { uniffiInitAsync } from "../../../../target/sdk-generated/typescript-wasm/binding";
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
  dispatchGenerated,
  browserPoolLocks(),
);
