import { CONTRACT_HASH, PROTOCOL_VERSION } from "./contract.gen.js";
import { dispatchGenerated } from "./dispatch.gen.js";
import { uniffiInitAsync } from "./index.js";
import type { WireMessage } from "./runtime/bridge/wire.js";
import { browserPoolLocks, WorkerHost } from "./runtime/bridge/worker/host.js";
import * as B from "./xmtp_sdk.js";

new WorkerHost(
  {
    postMessage: (message, transfer) =>
      self.postMessage(message, { transfer: transfer ?? [] }),
    onMessage: (handler) =>
      self.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      ),
    onExit: () => {},
    close: () => self.close(),
  },
  PROTOCOL_VERSION,
  CONTRACT_HASH,
  async () => {
    await uniffiInitAsync(new URL("./xmtp_sdk.wasm", import.meta.url));
  },
  dispatchGenerated,
  browserPoolLocks(true),
  B.prepareStorageForShutdown,
);
