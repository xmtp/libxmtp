import { expect, test } from "vitest";

import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire";

const CYCLES = 20;
const CONCURRENT = 32;
const DEADLINE_MS = 10_000;

function signal() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function within<T>(promise: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("callback barrier timed out")),
          DEADLINE_MS,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

function realWorker() {
  const worker = new Worker(
    new URL("./callback-lifetime.worker.ts", import.meta.url),
    { type: "module" },
  );
  let terminated = 0;
  let replies = 0;
  const workerError = signal();
  worker.addEventListener("error", (event) => {
    event.preventDefault();
    workerError.resolve();
  });
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      if (message.t === "callbackResult") replies++;
      worker.postMessage(message, transfer);
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      terminated++;
      worker.terminate();
    },
  };
  const session = new MainSession(endpoint, 1, "callback-lifetime");
  return {
    session,
    worker,
    terminated: () => terminated,
    replies: () => replies,
    workerError: workerError.promise,
  };
}

function count(object: object, field: string) {
  const value: unknown = Reflect.get(object, field);
  expect(value).toBeInstanceOf(Map);
  return (value as Map<unknown, unknown>).size;
}

const families = [
  ["Signer", "identity", CONCURRENT],
  ["Signer", "kind", CONCURRENT],
  ["Signer", "sign", CONCURRENT],
  ["CredentialSource", "credential", CONCURRENT],
  ["PreAuthenticate", "run", 1],
  ["EventListener", "onEvent", 1],
] as const;

// This proves the real browser transport. The final package matrix also has
// to call the generated SDK methods and exercise each owner's shutdown.
for (const [family, method, width] of families) {
  for (const mode of ["complete", "session-close", "worker-death"] as const) {
    test(`callback transport: ${family}.${method}, ${mode}, 20 cycles`, async () => {
      for (let cycle = 0; cycle < CYCLES; cycle++) {
        const { session, worker, terminated, replies, workerError } =
          realWorker();
        const allEntered = signal();
        const release = signal();
        const allFinished = signal();
        let active = 0;
        let entered = 0;
        let finished = 0;
        const calls: Promise<unknown>[] = [];
        try {
          await within(session.ready());
          for (let index = 0; index < width; index++) {
            const handle = session.callbacks.register(
              family,
              {
                async [method]() {
                  active++;
                  entered++;
                  if (entered === width) allEntered.resolve();
                  try {
                    await release.promise;
                    return "complete";
                  } finally {
                    active--;
                    finished++;
                    if (finished === width) allFinished.resolve();
                  }
                },
              },
              [method],
            );
            calls.push(
              session.call("callback", [handle.cb, method]).then(
                (value) => ({ value }),
                (error: unknown) => ({
                  code:
                    error instanceof Error
                      ? Reflect.get(error, "code")
                      : undefined,
                }),
              ),
            );
          }
          await within(allEntered.promise);
          expect(active).toBe(width);
          expect(count(session.callbacks, "targets")).toBe(width);
          expect(await within(session.call("counts", []))).toEqual({
            callbacks: width,
            handles: 0,
          });
          // Reentry must work with every callback still held.
          expect(await within(session.call("ping", []))).toBe("reentered");
          if (mode === "session-close") session.terminate();
          if (mode === "worker-death")
            await session.call("die", []).catch(() => {});
          if (mode !== "complete") {
            expect(await within(Promise.all(calls))).toEqual(
              Array.from({ length: width }, () => ({
                code: "WorkerTerminated",
              })),
            );
            expect(count(session.callbacks, "targets")).toBe(0);
            expect(active).toBe(width);
            expect(terminated()).toBe(1);
            if (mode === "worker-death") await within(workerError);
          }
          release.resolve();
          await within(allFinished.promise);
          expect(active).toBe(0);
          if (mode === "complete") {
            expect(await within(Promise.all(calls))).toEqual(
              Array.from({ length: width }, () => ({ value: "complete" })),
            );
            expect(await within(session.call("counts", []))).toEqual({
              callbacks: 0,
              handles: 0,
            });
            expect(replies()).toBe(width);
          } else {
            // A completed app callback must not send to a dead worker.
            await Promise.resolve();
            expect(replies()).toBe(0);
          }
          expect(count(session, "pending")).toBe(0);
          expect(count(session.callbacks, "targets")).toBe(0);
        } finally {
          release.resolve();
          session.terminate();
          worker.terminate();
        }
      }
    }, 120_000);
  }
}
