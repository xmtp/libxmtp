import { expect, it, vi } from "vitest";

import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  PoolLocks,
  WorkerHost,
  callWithPool,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";
import {
  pair,
  workerLockManager,
  heldPoolLocks,
  createUnencodableClient,
} from "./bridge-support";
export function registerCreateTests(): void {
  it("ends a created client before its pool lock is released when the encode fails", async () => {
    const { held, provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    let lockHeldAtEnd: boolean | undefined;
    const client = {
      end: async () => {
        lockHeldAtEnd = held.has("xmtp:client-pool");
      },
    };
    await createUnencodableClient(locks, client);
    expect(lockHeldAtEnd).toBe(true);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("ends the worker and keeps the pool lock when a created client that fails to encode cannot end", async () => {
    const held = new Set<string>();
    const first = workerLockManager(held);
    const locks = new PoolLocks(first.provider);
    const otherTab = new PoolLocks(workerLockManager(held).provider);
    const client = {
      end: async () => {
        throw new Error("close failed");
      },
    };
    const [main, worker] = pair();
    let terminateCalls = 0;
    main.terminate = () => {
      terminateCalls++;
    };
    const engine = new WorkerHost(
      worker,
      1,
      "unended",
      async () => {},
      (_key, _args, context) =>
        callWithPool(
          context.locks,
          "client-pool",
          true,
          async () => client,
          () =>
            context.registry.scope(() =>
              context.registry.add(client, "Client", undefined, () => {
                throw new Error("snapshot failed");
              }),
            ),
          () => false,
        ),
      locks,
    );
    const session = new MainSession(main, 1, "unended");
    await session.ready();
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await expect(session.call("Client.create", [])).rejects.toThrow(
        "snapshot failed",
      );
      expect(logged).toHaveBeenCalledWith(
        "client that failed to encode could not close",
        expect.objectContaining({ message: "close failed" }),
      );
    } finally {
      logged.mockRestore();
    }
    expect(engine.registry.size).toBe(0);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(worker.sent.some((message) => message.t === "fatal")).toBe(true);
    expect(terminateCalls).toBe(1);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.terminate();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("ends the worker and keeps the pool lock when a failed create leaves its store open", async () => {
    const held = new Set<string>();
    const first = workerLockManager(held);
    const locks = new PoolLocks(first.provider);
    const otherTab = new PoolLocks(workerLockManager(held).provider);
    const [main, worker] = pair();
    let terminateCalls = 0;
    main.terminate = () => {
      terminateCalls++;
    };
    new WorkerHost(
      worker,
      1,
      "left-open",
      async () => {},
      (_key, _args, context) =>
        callWithPool(
          context.locks,
          "client-pool",
          true,
          async () => {
            throw new Error("registration failed");
          },
          (result) => result,
          () => true,
        ),
      locks,
    );
    const session = new MainSession(main, 1, "left-open");
    await session.ready();
    await expect(session.call("Client.create", [])).rejects.toThrow(
      "registration failed",
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(worker.sent.some((message) => message.t === "fatal")).toBe(true);
    expect(terminateCalls).toBe(1);
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.terminate();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });
}
