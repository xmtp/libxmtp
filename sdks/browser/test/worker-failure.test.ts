import { XmtpError, type StreamCloseReason } from "@xmtp/browser-sdk";
import { afterEach, expect, test } from "vitest";

import { create } from "./helpers";

// The package starts its worker with `new Worker(...)`. Record each one so the
// test can stop it the way the browser does when a worker dies, and note when
// the main thread posts a stream read to it.
const OriginalWorker = globalThis.Worker;
const workers: Worker[] = [];
let readPosted = () => {};
globalThis.Worker = class extends OriginalWorker {
  constructor(url: string | URL, options?: WorkerOptions) {
    super(url, options);
    workers.push(this);
  }
  override postMessage(message: unknown, transfer?: unknown): void {
    if (Reflect.get(Object(message), "key") === "MessageReader.next")
      readPosted();
    super.postMessage(message, transfer as Transferable[]);
  }
};
afterEach(() => {
  globalThis.Worker = OriginalWorker;
});

function kill(worker: Worker): void {
  worker.terminate();
  worker.dispatchEvent(new Event("error"));
}

test("worker death fails a pending stream read with a typed error and end still resolves", async () => {
  const alix = await create();
  const group = await alix.conversations.createGroup([]);
  const reasons: StreamCloseReason[] = [];
  const stream = group.streamMessages({
    onClose: (reason) => reasons.push(reason),
  });
  await stream.ready();
  const posted = new Promise<void>((resolve) => (readPosted = resolve));
  const pending = stream.next();
  await posted;
  const worker = workers.at(-1);
  if (!worker) throw new Error("The package did not start a worker");
  kill(worker);

  const error = await pending.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(XmtpError.Unknown);
  expect((error as XmtpError).details).toStrictEqual({
    code: "Unknown",
    category: "lifecycle",
    retryable: false,
    message: "workerTerminated",
  });
  await expect.poll(() => reasons.length).toBe(1);
  expect(reasons[0]).toMatchObject({ kind: "failed" });
  expect((reasons[0] as { error: unknown }).error).toBe(error);

  // Ending the client and the stream settles without the dead worker.
  await stream.end();
  await expect(alix.end()).resolves.toBeUndefined();
  await expect(group.sendText("after death")).rejects.toBeInstanceOf(
    XmtpError.ClientClosed,
  );

  // The next client starts a new worker.
  const count = workers.length;
  const next = await create();
  expect(workers.length).toBe(count + 1);
  expect(await next.isRegistered()).toBe(true);
});
