import { expect, it, vi } from "vitest";

import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  PoolLocks,
  WorkerHost,
  callWithPool,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";
import { heldPoolLocks, pair } from "./bridge-support";

export function registerAdminTests(): void {
  for (const result of [17, undefined, { owner: 98 }]) {
    it(`releases an ordinary operation lease after ${JSON.stringify(result)}`, async () => {
      const { held, provider } = heldPoolLocks();
      const locks = new PoolLocks(provider);
      await locks.open("pool");
      locks.attachOwner(1, "pool");
      let finish!: () => void;
      const pending = new Promise<void>((resolve) => {
        finish = resolve;
      });
      let entered!: () => void;
      const started = new Promise<void>((resolve) => {
        entered = resolve;
      });
      const operation = callWithPool(
        locks,
        "pool",
        false,
        async () => {
          entered();
          await pending;
          return result;
        },
        (value) => value,
        () => false,
      );
      await started;
      locks.closeOwner(1);
      expect(held.has("xmtp:pool")).toBe(true);
      finish();
      expect(await operation).toEqual(result);
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      expect(held.has("xmtp:pool")).toBe(false);
    });
  }

  it("ends a collected admin while another admin keeps the pool", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const [main, worker] = pair();
    const engine = new WorkerHost(
      worker,
      1,
      "admin",
      async () => {},
      async () => {},
      locks,
    );
    const session = new MainSession(main, 1, "admin");
    await session.ready();
    const end = vi.fn(async () => {
      expect(held.has("xmtp:pool")).toBe(true);
    });
    const first = engine.registry.add({ end }, "StorageAdmin");
    const second = engine.registry.add({ end }, "StorageAdmin");
    for (const handle of [first, second]) {
      await locks.open("pool");
      locks.attachOwner(handle.owner, "pool");
    }
    main.postMessage({ t: "release", handles: [first.h] });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).toHaveBeenCalledTimes(1);
    expect(held.has("xmtp:pool")).toBe(true);
    expect(engine.registry.get(second)).toEqual({ end });
    main.postMessage({ t: "release", handles: [second.h] });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).toHaveBeenCalledTimes(2);
    expect(held.has("xmtp:pool")).toBe(false);
  });
  it("fences admin end and waits for accepted calls to start and settle", async () => {
    const [main, worker] = pair();
    let arrive!: () => void;
    const arrived = new Promise<void>((resolve) => {
      arrive = resolve;
    });
    let begin!: () => void;
    const beginning = new Promise<void>((resolve) => {
      begin = resolve;
    });
    let finish!: () => void;
    const finishing = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const end = vi.fn(async () => {});
    const engine = new WorkerHost(
      worker,
      1,
      "drain",
      async () => {},
      async (key, _args, context) => {
        if (key === "StorageAdmin.end") {
          await end();
          return;
        }
        arrive();
        await beginning;
        context.started?.();
        await finishing;
        return 23;
      },
    );
    const session = new MainSession(main, 1, "drain");
    await session.ready();
    const handle = engine.registry.add({ end }, "StorageAdmin");
    const call = session.call("StorageAdmin.fileCount", [], handle);
    await arrived;
    let ended = false;
    const closing = session.call("StorageAdmin.end", [], handle).then(() => {
      ended = true;
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).not.toHaveBeenCalled();
    await expect(
      session.call("StorageAdmin.fileCount", [], handle),
    ).rejects.toMatchObject({ code: "ClientClosed" });
    begin();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(end).toHaveBeenCalledTimes(1);
    expect(ended).toBe(false);
    finish();
    expect(await call).toBe(23);
    await closing;
    expect(ended).toBe(true);
  });

  it("ends an admin whose created handle cannot be delivered", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const [main, worker] = pair();
    const post = worker.postMessage.bind(worker);
    worker.postMessage = (message, transfer) => {
      if (message.t === "return") throw new Error("delivery failed");
      post(message, transfer);
    };
    const end = vi.fn(async () => {
      expect(held.has("xmtp:pool")).toBe(true);
    });
    const admin = { end };
    const engine = new WorkerHost(
      worker,
      1,
      "delivery",
      async () => {},
      async (_key, _args, context) => {
        const handle = await callWithPool(
          locks,
          "pool",
          true,
          () => admin,
          () => context.registry.add(admin, "StorageAdmin"),
          () => false,
        );
        context.createdOwner = (handle as { owner: number }).owner;
        return handle;
      },
      locks,
    );
    const session = new MainSession(main, 1, "delivery");
    await session.ready();
    await expect(session.call("StorageAdmin.open", [])).rejects.toThrow(
      "delivery failed",
    );
    expect(end).toHaveBeenCalledTimes(1);
    expect(engine.registry.size).toBe(0);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(held.has("xmtp:pool")).toBe(false);
  });
}
