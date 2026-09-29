import { expect, it } from "vitest";

import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.js";
import { host } from "./bridge-support";

function latch() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

// A read value that arrives while its owner's Client.end runs waits for the
// end. The generated Client.end fences the owner and unfences it on failure.
async function endWithReadInTransit(endFails: boolean) {
  const readHeld = latch();
  const readRelease = latch();
  const endEntered = latch();
  const endRelease = latch();
  const { engine, session } = host(async (key, _args, context) => {
    if (key === "MessageReader.next") {
      context.started?.();
      context.settled?.();
      readHeld.resolve();
      await readRelease.promise;
      return { id: "admitted" };
    }
    if (key === "Client.end") {
      endEntered.resolve();
      await endRelease.promise;
      if (endFails) throw new Error("end failed");
    }
    return undefined;
  });
  await session.ready();
  const clientHandle = engine.registry.add({}, "Client", undefined, () => ({
    clientKey: 7n,
  }));
  const client = new Client(session, clientHandle);
  const readerHandle = engine.registry.add(
    {},
    "MessageReader",
    clientHandle.owner,
  );
  let settled = false;
  const read = session.call("MessageReader.next", [], readerHandle).then(
    (value) => ({ value }),
    (error: unknown) => ({ error }),
  );
  void read.finally(() => {
    settled = true;
  });
  await readHeld.promise;
  const end = client.end().then(
    () => ({ ended: true }),
    (error: unknown) => ({ error }),
  );
  await endEntered.promise;
  readRelease.resolve();
  await new Promise<void>((resolve) => setTimeout(resolve, 10));
  expect(settled).toBe(false);
  endRelease.resolve();
  return { end: await end, read: await read };
}

export function registerEndingTests(): void {
  // verifies: PROC-028
  it("delivers a read held during a Client.end that fails", async () => {
    const { end, read } = await endWithReadInTransit(true);
    expect(end).toMatchObject({ error: { message: "end failed" } });
    expect(read).toEqual({ value: { id: "admitted" } });
  });

  // verifies: PROC-028
  it("abandons a read held during a Client.end that succeeds", async () => {
    const { end, read } = await endWithReadInTransit(false);
    expect(end).toEqual({ ended: true });
    expect(read).toMatchObject({ error: { code: "ClientClosed" } });
  });
}
