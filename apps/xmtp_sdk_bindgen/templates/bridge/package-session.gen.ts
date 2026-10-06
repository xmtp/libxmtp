import { CONTRACT_HASH, PROTOCOL_VERSION } from "./contract.gen.js";
import { setLogSink as setWorkerLogSink } from "./proxy.gen.js";
import { logSinkSetter } from "./runtime/bridge/main/log-sink.js";
import type { MainSession } from "./runtime/bridge/main/session.js";
import { WorkerSessions } from "./runtime/bridge/main/worker-sessions.js";
import type { WireMessage } from "./runtime/bridge/wire.js";
import type { LogSink } from "./xmtp_sdk.js";

const updateLogSink = logSinkSetter<LogSink>(loggingInWorker, setWorkerLogSink);

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
  (session) => updateLogSink.initialize(session),
);

export function createInWorker<T>(
  create: (session: MainSession) => Promise<T>,
): Promise<T> {
  return sessions.create(create);
}

/** Migration cannot share a live storage owner. Its worker ends before return. */
export function migrateInWorker<T>(call: (session: MainSession) => Promise<T>): Promise<T> {
  return sessions.runExclusive(call);
}

/** Logging setup does not own a client. Keep the initial worker for its first create. */
export async function loggingInWorker<T>(
  call: (session: MainSession) => Promise<T>,
): Promise<T> {
  return call(await sessions.get());
}

/** Keep accepted logging configuration separate from worker ownership. */
export function initLoggingInWorker<Options>(
  options: Options,
  configure: (session: MainSession, options: Options) => Promise<void>,
): Promise<void> {
  const snapshot = structuredClone(options);
  return updateLogSink.configure((session) => configure(session, snapshot));
}

export function setPackageLogSink(sink?: LogSink): Promise<void> {
  return updateLogSink(sink);
}
