import { CONTRACT_HASH, PROTOCOL_VERSION } from "./contract.gen.js";
import { dispatchGenerated } from "./dispatch.gen.js";
import { uniffiInitAsync } from "./binding.js";
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
  async (lifetimeLock) => {
    if (!lifetimeLock) throw new Error("Missing worker lifetime lock");
    // Hold the marker before opening OPFS. Its release confirms worker exit.
    await new Promise<void>((resolve, reject) => {
      void navigator.locks
        .request(lifetimeLock, async () => {
          resolve();
          await new Promise<void>(() => {});
        })
        .catch(reject);
    });
    await uniffiInitAsync(new URL("./xmtp_sdk.wasm", import.meta.url));
  },
  dispatchGenerated,
  browserPoolLocks(true),
  B.prepareStorageForShutdown,
);
