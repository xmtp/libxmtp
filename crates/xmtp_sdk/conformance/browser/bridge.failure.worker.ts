import { WorkerHost } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire";

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

new WorkerHost(endpoint, 1, "browser-failure", async () => {}, async (key) => {
  if (key === "wait") return new Promise<never>(() => {});
  if (key === "fail") {
    void Promise.reject(new Error("background worker failure"));
    return undefined;
  }
  throw new Error(`unknown test call ${key}`);
});
