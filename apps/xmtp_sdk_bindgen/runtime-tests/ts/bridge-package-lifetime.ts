import { expect, it, vi } from "vitest";

import { WorkerSessions } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/worker-sessions";
import type { HandleWire } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import {
  WorkerHost,
  PoolLocks,
  callWithPool,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host";
import { Endpoint, pair, workerLockManager } from "./bridge-support";

function gate() {
  let release!: () => void;
  return {
    promise: new Promise<void>((resolve) => {
      release = resolve;
    }),
    release: () => release(),
  };
}
const turn = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

export function registerPackageLifetimeTests(): void {
  it("reserves both package creations before their first await and cleans failed factories", async () => {
    const endpoint = new Endpoint();
    const terminate = vi.fn();
    endpoint.terminate = terminate;
    const sessions = new WorkerSessions(() => endpoint, 3, "package");
    const firstGate = gate();
    const secondGate = gate();
    const first = sessions.create(async () => {
      await firstGate.promise;
      throw new Error("first failed");
    });
    const second = sessions.create(async () => {
      await secondGate.promise;
      throw new Error("second failed");
    });
    const rejectedFirst = expect(first).rejects.toThrow("first failed");
    const rejectedSecond = expect(second).rejects.toThrow("second failed");
    endpoint.emitRaw({ t: "ready", epoch: 1 });
    endpoint.emitRaw({ t: "idle", revision: 0 });
    await turn();
    expect(terminate).not.toHaveBeenCalled();
    firstGate.release();
    await rejectedFirst;
    expect(terminate).not.toHaveBeenCalled();
    secondGate.release();
    await rejectedSecond;
    expect(terminate).toHaveBeenCalledTimes(1);
  });

  it("ignores stale idle reports while a new package call is accepted", async () => {
    const endpoint = new Endpoint();
    const terminate = vi.fn();
    endpoint.terminate = terminate;
    const sessions = new WorkerSessions(() => endpoint, 3, "package");
    const created = sessions.create((session) => session.call("open", []));
    endpoint.emitRaw({ t: "ready", epoch: 1 });
    await turn();
    const call = endpoint.sent.findLast((message) => message.t === "call");
    if (call?.t !== "call") throw new Error("no create call");
    endpoint.emitRaw({ t: "idle", revision: 0 });
    endpoint.emitRaw({ t: "return", id: call.id, value: 42 });
    expect(await created).toBe(42);
    expect(terminate).not.toHaveBeenCalled();
    endpoint.emitRaw({ t: "idle", revision: call.revision });
    expect(terminate).toHaveBeenCalledTimes(1);
  });

  it("keeps the last pool lock through collected-owner cleanup and terminates the endpoint", async () => {
    const held = new Set<string>();
    const lockManager = workerLockManager(held);
    const locks = new PoolLocks(lockManager.provider, true);
    const [main, worker] = pair();
    const terminate = vi.fn(() => lockManager.terminate());
    main.terminate = terminate;
    const closing = gate();
    const entered = gate();
    const end = vi.fn(async () => {
      entered.release();
      await closing.promise;
    });
    const prepareIdle = vi.fn();
    new WorkerHost(
      worker,
      3,
      "package",
      async () => {},
      async (_key, _args, context) =>
        callWithPool(
          locks,
          "pool",
          true,
          () => ({ end }),
          (root) => context.registry.add(root as object, "StorageAdmin"),
          () => false,
        ),
      locks,
      prepareIdle,
    );
    const sessions = new WorkerSessions(() => main, 3, "package");
    const { session, handle } = await sessions.create(async (session) => ({
      session,
      handle: (await session.call("open", [])) as HandleWire,
    }));
    session.release([handle.h]);
    await entered.promise;
    expect(held.has("xmtp:pool")).toBe(true);
    expect(terminate).not.toHaveBeenCalled();
    // The startup idle report is allowed; cleanup must produce one more.
    const prepared = prepareIdle.mock.calls.length;
    closing.release();
    await turn();
    expect(end).toHaveBeenCalledTimes(1);
    expect(prepareIdle.mock.calls.length).toBe(prepared + 1);
    expect(terminate).toHaveBeenCalledTimes(1);
    expect(held.has("xmtp:pool")).toBe(false);
    expect(session.isTerminated).toBe(true);
  });

  // verifies: LOG-002, LOG-004, LOG-008
  it("keeps a managed worker through a held log and its queued successor", async () => {
    const [main, worker] = pair();
    const stopped = vi.fn();
    main.terminate = stopped;
    let delivery: Promise<void> = Promise.resolve();
    const host = new WorkerHost(
      worker,
      3,
      "logs",
      async () => {},
      async (_key, _args, context) =>
        context.registry.add({ end: async () => {} }, "StorageAdmin"),
      undefined,
      () => delivery,
    );
    const sessions = new WorkerSessions(() => main, 3, "logs");
    const { session, handle } = await sessions.create(async (session) => ({
      session,
      handle: (await session.call("open", [])) as HandleWire,
    }));
    const first = gate();
    const second = gate();
    const entered = gate();
    let calls = 0;
    const sink = session.callbacks.register(
      "LogSink",
      {
        async log() {
          calls++;
          if (calls === 1) {
            entered.release();
            await first.promise;
          } else await second.promise;
        },
      },
      ["log"],
    );
    delivery = (async () => {
      await host.callbacks.invoke(sink.cb, "log", []);
      await host.callbacks.invoke(sink.cb, "log", []);
    })();
    try {
      await entered.promise;
      session.release([handle.h]);
      await turn();
      expect(stopped).not.toHaveBeenCalled();
      first.release();
      await turn();
      expect(calls).toBe(2);
      expect(stopped).not.toHaveBeenCalled();
      second.release();
      await turn();
      expect(stopped).toHaveBeenCalledTimes(1);
    } finally {
      first.release();
      second.release();
      sessions.terminate();
    }
  });

  it("invalidates a held idle preparation when a package owner appears", async () => {
    const [main, worker] = pair();
    const stopped = vi.fn();
    main.terminate = stopped;
    const prepared = gate();
    const preparing = gate();
    let preparations = 0;
    new WorkerHost(
      worker,
      3,
      "idle",
      async () => {},
      async (_key, _args, context) =>
        context.registry.add({ end: async () => {} }, "StorageAdmin"),
      undefined,
      async () => {
        preparations++;
        if (preparations === 1) {
          preparing.release();
          await prepared.promise;
        }
      },
    );
    const sessions = new WorkerSessions(() => main, 3, "idle");
    const session = await sessions.get();
    await preparing.promise;
    const handle = (await sessions.create((session) =>
      session.call("open", []),
    )) as HandleWire;
    prepared.release();
    await turn();
    expect(stopped).not.toHaveBeenCalled();
    session.release([handle.h]);
    await turn();
    expect(stopped).toHaveBeenCalledTimes(1);
    expect(preparations).toBe(2);
  });

  it("repeats idle preparation instead of sending an obsolete revision", async () => {
    const [main, worker] = pair();
    const stopped = vi.fn();
    main.terminate = stopped;
    const first = gate();
    const second = gate();
    const enteredFirst = gate();
    const enteredSecond = gate();
    let preparations = 0;
    new WorkerHost(
      worker,
      3,
      "revisions",
      async () => {},
      async () => 42,
      undefined,
      async () => {
        preparations++;
        if (preparations === 1) {
          enteredFirst.release();
          await first.promise;
        } else {
          enteredSecond.release();
          await second.promise;
        }
      },
    );
    const sessions = new WorkerSessions(() => main, 3, "revisions");
    await sessions.get();
    await enteredFirst.promise;
    expect(await sessions.create((session) => session.call("ping", []))).toBe(
      42,
    );
    first.release();
    await enteredSecond.promise;
    expect(worker.sent.filter((message) => message.t === "idle")).toEqual([]);
    expect(stopped).not.toHaveBeenCalled();
    second.release();
    await turn();
    expect(stopped).toHaveBeenCalledTimes(1);
  });

  it("retains an idle pool lock until its actual worker terminates", async () => {
    const held = new Set<string>();
    const manager = workerLockManager(held);
    const locks = new PoolLocks(manager.provider, true);
    await locks.open("pool");
    locks.close("pool");
    await turn();
    expect(held.has("xmtp:pool")).toBe(true);
    manager.terminate();
    await turn();
    expect(held.has("xmtp:pool")).toBe(false);
  });
  it("waits for actual termination before a reentrant replacement starts", async () => {
    const endpoints: Endpoint[] = [];
    const stopped = gate();
    let replacement: Promise<number> | undefined;
    const sessions = new WorkerSessions(
      () => {
        const endpoint = new Endpoint();
        endpoints.push(endpoint);
        if (endpoints.length === 1)
          endpoint.terminate = () => {
            replacement = sessions.create(async () => 7);
            return stopped.promise;
          };
        return endpoint;
      },
      3,
      "handoff",
    );
    const first = sessions.create(async () => 3);
    endpoints[0].emitRaw({ t: "ready", epoch: 1 });
    endpoints[0].emitRaw({ t: "idle", revision: 0 });
    expect(await first).toBe(3);
    expect(endpoints).toHaveLength(1);
    stopped.release();
    await turn();
    expect(endpoints).toHaveLength(2);
    endpoints[1].emitRaw({ t: "ready", epoch: 2 });
    endpoints[1].emitRaw({ t: "idle", revision: 0 });
    expect(await replacement).toBe(7);
  });
}
