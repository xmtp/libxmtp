import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire";

// verifies: P55
export async function checkWorkerFailure(): Promise<void> {
  const worker = new Worker(new URL("./bridge.failure.worker.ts", import.meta.url), {
    type: "module",
  });
  let fatal = false;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, transfer);
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) => {
        if (event.data.t === "fatal") fatal = true;
        handler(event.data);
      });
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  const session = new MainSession(endpoint, 1, "browser-failure");
  try {
    await session.ready();
    const waiting = session.call("wait", []);
    await session.call("fail", []).catch(() => {});
    let code: unknown;
    try {
      await Promise.race([
        waiting,
        new Promise<never>((_resolve, reject) =>
          setTimeout(() => reject(new Error("worker failure timed out")), 5000),
        ),
      ]);
    } catch (error) {
      code = error instanceof Error ? Reflect.get(error, "code") : undefined;
    }
    if (!fatal || code !== "workerTerminated") {
      throw new Error(`worker failure was not fatal: ${String(code)}`);
    }
  } finally {
    worker.terminate();
  }
}
