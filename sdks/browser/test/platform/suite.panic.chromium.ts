import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/contract.gen";
import { MainSession } from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-bridge-panic-fixture/typescript-wasm/runtime/bridge/wire";

export async function checkRealWasmTrap(): Promise<void> {
  const worker = new Worker(
    new URL("./suite.panic.worker.ts", import.meta.url),
    {
      type: "module",
    },
  );
  let sawFatal = false;
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) => {
        if (event.data.t === "fatal") sawFatal = true;
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
  const session = new MainSession(endpoint, PROTOCOL_VERSION, CONTRACT_HASH);
  try {
    await session.ready();
    const waiting = session.call("__conformanceWait", []);
    void waiting.catch(() => {});
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    const trap = session.call("bridgeTestPanic", []);
    const failures = await Promise.race([
      Promise.allSettled([waiting, trap]),
      new Promise<never>((_resolve, reject) =>
        setTimeout(
          () => reject(new Error("WASM trap did not stop the worker")),
          5000,
        ),
      ),
    ]);
    if (
      !sawFatal ||
      failures.some(
        (result) =>
          result.status !== "rejected" ||
          Reflect.get(result.reason, "code") !== "WorkerTerminated",
      )
    ) {
      throw new Error("real WASM trap did not settle every pending call");
    }
  } finally {
    worker.terminate();
  }
}
