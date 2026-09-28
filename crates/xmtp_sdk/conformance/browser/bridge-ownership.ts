import { expect, it, vi } from "vitest";

import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session.js";
import {
  PoolLocks,
  WorkerHost,
  type LockProvider,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.js";
import {
  pair,
  TestProxy,
  workerLockManager,
  heldPoolLocks,
} from "./bridge-support";
export function registerOwnershipTests(): void {
  it("ends a client before an owner release frees its pool lock", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const [main, worker] = pair();
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async () => undefined,
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    let lockHeldAtEnd: boolean | undefined;
    const end = vi.fn(async () => {
      lockHeldAtEnd = held.has("xmtp:client-pool");
    });
    const handle = engine.registry.add({ end }, "Client");
    locks.attachOwner(handle.owner, "client-pool");
    main.postMessage({
      t: "release",
      handles: [handle.h],
      owners: [handle.owner],
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).toHaveBeenCalledTimes(1);
    expect(lockHeldAtEnd).toBe(true);
    expect(engine.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("releases the pool lock after Client.end without a second end", async () => {
    const { provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const [main, worker] = pair();
    const end = vi.fn(async () => {});
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async (key, _args, context) => {
        if (key === "Client.end" && context.target)
          await Reflect.apply(end, context.target, []);
      },
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    const handle = engine.registry.add({ end }, "Client");
    locks.attachOwner(handle.owner, "client-pool");
    const client = new TestProxy(session, handle);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    await client.end();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).toHaveBeenCalledTimes(1);
    expect(engine.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("ends an owner only through the Client.end of its own client", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const [main, worker] = pair();
    // This dispatch does not check the target type, so the host check alone
    // must keep the owner open.
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async (key, _args, context) => {
        if (key !== "Client.end" || !context.target) return;
        const end: unknown = Reflect.get(context.target, "end");
        if (typeof end === "function")
          await Reflect.apply(end, context.target, []);
      },
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    const lockHeldAtEnd: boolean[] = [];
    const clientEnd = vi.fn(async () => {
      lockHeldAtEnd.push(held.has("xmtp:client-pool"));
    });
    const readerEnd = vi.fn(async () => {});
    const client = engine.registry.add({ end: clientEnd }, "Client");
    const reader = engine.registry.add(
      { end: readerEnd },
      "MessageReader",
      client.owner,
    );
    locks.attachOwner(client.owner, "client-pool");
    await session.call("Client.end", [], reader);
    expect(readerEnd).toHaveBeenCalledTimes(1);
    main.postMessage({
      t: "release",
      handles: [client.h, reader.h],
      owners: [client.owner],
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(clientEnd).toHaveBeenCalledTimes(1);
    expect(lockHeldAtEnd).toEqual([true]);
    expect(engine.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("releases the pool owner when the Client handle is collected", async () => {
    const held = new Set<string>();
    const provider: LockProvider = {
      async request(name, _options, callback) {
        if (held.has(name)) return callback(null);
        held.add(name);
        try {
          await callback({});
        } finally {
          held.delete(name);
        }
      },
    };
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const [main, worker] = pair();
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async () => undefined,
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    const handle = engine.registry.add({ end: async () => {} }, "Client");
    locks.attachOwner(handle.owner, "client-pool");
    const client = new TestProxy(session, handle);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    client.release();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(engine.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("keeps a pool locked until the worker terminates after collected Client.end rejects", async () => {
    const held = new Set<string>();
    const first = workerLockManager(held);
    const locks = new PoolLocks(first.provider);
    const otherTab = new PoolLocks(workerLockManager(held).provider);
    const [main, worker] = pair();
    const engine = new WorkerHost(
      worker,
      1,
      "pool",
      async () => {},
      async () => undefined,
      locks,
    );
    const session = new MainSession(main, 1, "pool");
    await session.ready();
    await locks.open("client-pool");
    const handle = engine.registry.add(
      { end: async () => Promise.reject(new Error("close failed")) },
      "Client",
    );
    locks.attachOwner(handle.owner, "client-pool");
    const client = new TestProxy(session, handle);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    client.release();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(worker.sent.some((message) => message.t === "fatal")).toBe(true);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.terminate();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("holds acquired pool locks until a fatal worker terminates", async () => {
    const held = new Set<string>();
    const first = workerLockManager(held);
    const locks = new PoolLocks(first.provider);
    const otherWorker = new PoolLocks(workerLockManager(held).provider);
    const [main, worker] = pair();
    let terminateCalls = 0;
    main.terminate = () => {
      terminateCalls++;
    };
    const engine = new WorkerHost(
      worker,
      1,
      "fatal",
      async () => {},
      async () => undefined,
      locks,
    );
    const session = new MainSession(main, 1, "fatal");
    await session.ready();
    await locks.open("client-pool");
    const handle = engine.registry.add({ end: async () => {} }, "Client");
    locks.attachOwner(handle.owner, "client-pool");
    const pending = locks.open("pending-pool");
    engine.fatal(new Error("panic"));
    await expect(pending).rejects.toMatchObject({ code: "WorkerTerminated" });
    locks.closeOwner(handle.owner);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(terminateCalls).toBe(1);
    await expect(otherWorker.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.terminate();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherWorker.open("client-pool");
    otherWorker.close("client-pool");
  });
}
