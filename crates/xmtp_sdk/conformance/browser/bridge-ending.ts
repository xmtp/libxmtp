import { expect, it } from "vitest";

import { Client } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen.js";
import { RemoteObject } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/remote-object.js";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  PoolLocks,
  WorkerHost,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";
import { registerEventEndingTests } from "./bridge-event-ending";
import { heldPoolLocks, host, pair } from "./bridge-support";

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

class EventReaderProxy extends RemoteObject {
  next(): Promise<unknown> {
    return this.call("EventReader.next", []);
  }
}

export function registerEndingTests(): void {
  registerEventEndingTests();
  it("ends in-flight, queued, and later event reads after a successful close", async () => {
    const entered = latch();
    const release = latch();
    let reads = 0;
    const { engine, session } = host(async (key, _args, context) => {
      if (key === "EventReader.next") {
        reads++;
        context.started?.();
        context.settled?.();
        entered.resolve();
        await release.promise;
        return { kind: "conversation.joined" };
      }
      return undefined;
    });
    await session.ready();
    const owner = engine.registry.add(
      { end: () => Promise.resolve() },
      "Client",
    );
    const reader = new EventReaderProxy(
      session,
      engine.registry.add({}, "EventReader", owner.owner),
    );
    const first = reader.next();
    const queued = reader.next();
    await entered.promise;
    session.fenceOwner(owner.owner);
    session.closeOwner(owner.owner, []);
    await expect(first).resolves.toBeUndefined();
    await expect(queued).resolves.toBeUndefined();
    await expect(reader.next()).resolves.toBeUndefined();
    expect(reads).toBe(1);
    release.resolve();
    session.terminate();
  });

  for (const outcome of ["rollback", "death"] as const) {
    it(`settles an event read started during close on ${outcome}`, async () => {
      let reads = 0;
      const { engine, session } = host(async (key) => {
        if (key === "EventReader.next") {
          reads++;
          return { kind: "conversation.joined" };
        }
        return undefined;
      });
      await session.ready();
      const owner = engine.registry.add({}, "Client");
      const reader = new EventReaderProxy(
        session,
        engine.registry.add({}, "EventReader", owner.owner),
      );
      session.fenceOwner(owner.owner);
      const read = reader.next().then(
        (value) => ({ value }),
        (error: unknown) => ({ error }),
      );
      // This round trip runs after the queued read reaches the close fence.
      await session.call("barrier", []);
      expect(reads).toBe(0);
      if (outcome === "rollback") {
        session.unfenceOwner(owner.owner);
        expect(await read).toEqual({ value: { kind: "conversation.joined" } });
        expect(reads).toBe(1);
      } else {
        session.terminate();
        expect(await read).toMatchObject({
          error: { code: "WorkerTerminated" },
        });
        expect(reads).toBe(0);
      }
      session.terminate();
    });
  }

  // A signer callback may end its own client. The end must not wait for the
  // call that is parked on that callback, but the storage lock stays held
  // until that call finishes.
  it("ends a client from a signer callback of a running call", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const [main, worker] = pair();
    const finishRevoke = latch();
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async (key, args, context) => {
        if (key === "Client.revokeInstallations") {
          context.started?.();
          const signer = args[0] as { cb: number };
          await context.callbacks.invoke(signer.cb, "sign", []);
          await finishRevoke.promise;
          context.settled?.();
          return "revoked";
        }
        return undefined;
      },
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    const clientHandle = engine.registry.add({}, "Client", undefined, () => ({
      clientKey: 7n,
    }));
    locks.attachOwner(clientHandle.owner, "client-pool");
    const client = new Client(session, clientHandle);
    let ended: string | undefined;
    const signer = session.callbacks.register(
      "Signer",
      {
        sign: async () => {
          ended = await Promise.race([
            client.end().then(() => "ended"),
            new Promise<string>((resolve) =>
              setTimeout(() => resolve("end still pending"), 500),
            ),
          ]);
          return "signature";
        },
      },
      ["sign"],
    );
    const revoke = session.call(
      "Client.revokeInstallations",
      [signer],
      clientHandle,
    );
    await expect.poll(() => ended, { timeout: 2000 }).toBeDefined();
    expect(ended).toBe("ended");
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    expect(held.has("xmtp:client-pool")).toBe(true);
    finishRevoke.resolve();
    await expect(revoke).resolves.toBe("revoked");
    await expect.poll(() => held.has("xmtp:client-pool")).toBe(false);
  });

  // A queued read that is aborted settles at once and is never posted.
  it("settles an aborted queued read without posting it", async () => {
    let reads = 0;
    const { engine, session } = host(async (key) => {
      if (key === "MessageReader.next") {
        reads++;
        await new Promise<void>(() => {});
      }
      return undefined;
    });
    await session.ready();
    const owner = engine.registry.add({}, "Client");
    const reader = engine.registry.add({}, "MessageReader", owner.owner);
    void session.call("MessageReader.next", [], reader).catch(() => {});
    const abort = new AbortController();
    const second = session
      .call("MessageReader.next", [], reader, abort.signal)
      .then(
        () => "resolved",
        (error: unknown) => error,
      );
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    abort.abort();
    const outcome = await Promise.race([
      second,
      new Promise<string>((resolve) =>
        setTimeout(() => resolve("pending"), 50),
      ),
    ]);
    expect(outcome).toMatchObject({ code: "Cancelled" });
    expect(reads).toBe(1);
  });

  // A read queued behind an aborted read still waits for the read before it.
  it("keeps a later read behind the read before an aborted read", async () => {
    const release = latch();
    let reads = 0;
    const { engine, session } = host(async (key) => {
      if (key === "MessageReader.next") {
        reads++;
        if (reads === 1) await release.promise;
        return { id: `read ${reads}` };
      }
      return undefined;
    });
    await session.ready();
    const owner = engine.registry.add({}, "Client");
    const reader = engine.registry.add({}, "MessageReader", owner.owner);
    const first = session.call("MessageReader.next", [], reader);
    const abort = new AbortController();
    const second = session
      .call("MessageReader.next", [], reader, abort.signal)
      .catch((error: unknown) => error);
    abort.abort();
    expect(await second).toMatchObject({ code: "Cancelled" });
    const third = session.call("MessageReader.next", [], reader);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
    expect(reads).toBe(1);
    release.resolve();
    expect(await first).toEqual({ id: "read 1" });
    expect(await third).toEqual({ id: "read 2" });
    expect(reads).toBe(2);
  });

  // Worker death fails a queued read with the worker's error.
  it("rejects a queued read with WorkerTerminated when the worker ends", async () => {
    const { engine, session } = host(async (key) => {
      if (key === "MessageReader.next") await new Promise<void>(() => {});
      return undefined;
    });
    await session.ready();
    const owner = engine.registry.add({}, "Client");
    const reader = engine.registry.add({}, "MessageReader", owner.owner);
    const outcome = (read: Promise<unknown>) =>
      read.then(
        () => "resolved",
        (error: unknown) => error,
      );
    const first = outcome(session.call("MessageReader.next", [], reader));
    const second = outcome(session.call("MessageReader.next", [], reader));
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    session.terminate();
    expect(await first).toMatchObject({ code: "WorkerTerminated" });
    expect(await second).toMatchObject({ code: "WorkerTerminated" });
  });

  // verifies: PROC-052
  it("posts a second read only after the first read settles on the main thread", async () => {
    const readHeld = latch();
    const readRelease = latch();
    let reads = 0;
    const { engine, session } = host(async (key, _args, context) => {
      if (key === "MessageReader.next") {
        reads++;
        context.started?.();
        context.settled?.();
        readHeld.resolve();
        await readRelease.promise;
        return { id: `read ${reads}` };
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
    const settle = (read: Promise<unknown>) =>
      read.then(
        (value) => ({ value }),
        (error: unknown) => ({ error }),
      );
    const first = settle(session.call("MessageReader.next", [], readerHandle));
    const second = settle(session.call("MessageReader.next", [], readerHandle));
    await readHeld.promise;
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    expect(reads).toBe(1);
    const end = client.end();
    readRelease.resolve();
    await end;
    expect(await first).toMatchObject({ error: { code: "ClientClosed" } });
    expect(await second).toMatchObject({ error: { code: "ClientClosed" } });
    expect(reads).toBe(1);
  });

  // verifies: PROC-052
  it("delivers a read held during a Client.end that fails", async () => {
    const { end, read } = await endWithReadInTransit(true);
    expect(end).toMatchObject({ error: { message: "end failed" } });
    expect(read).toEqual({ value: { id: "admitted" } });
  });

  // verifies: PROC-052
  it("abandons a read held during a Client.end that succeeds", async () => {
    const { end, read } = await endWithReadInTransit(false);
    expect(end).toEqual({ ended: true });
    expect(read).toMatchObject({ error: { code: "ClientClosed" } });
  });
}
