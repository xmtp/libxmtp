import type {
  WireEndpoint,
  WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire";
import { WorkerHost } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host";

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

const host = new WorkerHost(
  endpoint,
  1,
  "callback-lifetime",
  async () => {},
  async (key, args, context) => {
    if (key === "ping") return "reentered";
    if (key === "counts") {
      // Read real production state. This fixture adds no shipped diagnostics.
      const pending = Reflect.get(context.callbacks, "pending") as Map<
        number,
        unknown
      >;
      return { callbacks: pending.size, handles: host.registry.size };
    }
    if (key === "die") {
      void Promise.reject(new Error("callback lifetime worker death"));
      return undefined;
    }
    if (key === "callback") {
      const [cb, method] = args as [number, string];
      try {
        return await context.callbacks.invoke(cb, method, []);
      } finally {
        context.callbacks.drop(cb);
      }
    }
    throw new Error(`Unknown lifetime fixture call: ${key}`);
  },
);
