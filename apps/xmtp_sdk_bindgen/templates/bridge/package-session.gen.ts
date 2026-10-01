import { CONTRACT_HASH, PROTOCOL_VERSION } from "./contract.gen.js";
import type { MainSession } from "./runtime/bridge/main/session.js";
import { WorkerSessions } from "./runtime/bridge/main/worker-sessions.js";
import type { WireMessage } from "./runtime/bridge/wire.js";

// One worker generation for all package client and admin factories.
const sessions = new WorkerSessions(
  () => {
    const lifetimeLock = `xmtp-worker:${crypto.randomUUID()}`;
    const worker = new Worker(
      new URL("./worker-entry.gen.js", import.meta.url),
      { type: "module" },
    );
    return {
      postMessage: (message, transfer) =>
        worker.postMessage(
          message.t === "hello" ? { ...message, lifetimeLock } : message,
          transfer ?? [],
        ),
      onMessage: (handler) =>
        worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
          handler(event.data),
        ),
      onExit: (handler) => {
        worker.addEventListener("error", handler);
        worker.addEventListener("messageerror", handler);
      },
      terminate: () => {
        worker.terminate();
        return navigator.locks.request(lifetimeLock, () => {});
      },
    };
  },
  PROTOCOL_VERSION,
  CONTRACT_HASH,
);

export function createInWorker<T>(
  create: (session: MainSession) => Promise<T>,
): Promise<T> {
  return sessions.create(create);
}

/** Logging setup does not own a client. Keep the initial worker for its first create. */
export async function loggingInWorker<T>(
  call: (session: MainSession) => Promise<T>,
): Promise<T> {
  return call(await sessions.get());
}
