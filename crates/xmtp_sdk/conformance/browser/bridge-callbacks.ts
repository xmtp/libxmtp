import { expect, it, vi } from "vitest";

import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  bridgeError,
  encodeError,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import {
  WorkerCallbacks,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/callback-stub.js";
import {
  PoolLocks,
  WorkerHost,
  poolName,
  type LockProvider,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";
import { pair, host, withoutUnhandledRejections } from "./bridge-support";
export function registerCallbacksTests(): void {
  it("keeps an app callback error named AbortError as an app failure", async () => {
    const failure = Object.assign(new Error("callback failed"), {
      name: "AbortError",
    });
    const { session } = host(async (_key, _args, context) =>
      context.callbacks.invoke(1, "sign", []),
    );
    session.callbacks.register(
      "Signer",
      {
        sign: () => {
          throw failure;
        },
      },
      ["sign"],
    );
    await session.ready();
    await expect(session.call("sign", [])).rejects.toMatchObject({
      variant: "AbortError",
      code: "Unknown",
      category: 10,
      retryable: false,
      message: "callback failed",
    });
  });

  it("reentrant_signer_completes", async () => {
    const { session, engine } = host(async (key, _args, context) => {
      if (key === "inner") return "inner result";
      return context.callbacks.invoke(1, "sign", []);
    });
    session.callbacks.register(
      "Signer",
      { sign: async () => session.call("inner", []) },
      ["sign"],
    );
    await session.ready();
    await expect(session.call("outer", [])).resolves.toBe("inner result");
    expect(engine.registry.size).toBe(0);
  });

  it("drops a callback reply after the worker endpoint closes", async () => {
    await withoutUnhandledRejections(async () => {
      for (const exits of [false, true]) {
        const { main, session } = host(async (_key, _args, context) =>
          context.callbacks.invoke(1, "sign", []),
        );
        let finish: (value: string) => void = () => {};
        const started = new Promise<void>((resolve) => {
          session.callbacks.register(
            "Signer",
            {
              sign: () => {
                resolve();
                return new Promise<string>((done) => (finish = done));
              },
            },
            ["sign"],
          );
        });
        await session.ready();
        const call = session.call("outer", []).catch((error: unknown) => error);
        await started;
        let attempts = 0;
        main.postMessage = () => {
          attempts++;
          throw new Error("endpoint closed");
        };
        if (exits) main.exit();
        finish("signed");
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
        // An exit settles the call. A silent close leaves the call to the
        // application, as the worker can never answer it.
        if (exits)
          await expect(call).resolves.toMatchObject({
            code: "WorkerTerminated",
          });
        expect(attempts).toBe(exits ? 0 : 1);
      }
    });
  });

  it("sends an error for a callback reply that cannot be cloned", async () => {
    await withoutUnhandledRejections(async () => {
      const { session } = host(async (key, _args, context) =>
        context.callbacks.invoke(1, key, []),
      );
      const detailed = Object.assign(new Error("signer failed"), {
        tag: "Signer",
        inner: [() => undefined],
      });
      session.callbacks.register(
        "Signer",
        {
          sign: async () => () => "not cloneable",
          kind: async () => {
            throw detailed;
          },
        },
        ["kind", "sign"],
      );
      await session.ready();
      await expect(session.call("sign", [])).rejects.toMatchObject({
        variant: "DataCloneError",
      });
      await expect(session.call("kind", [])).rejects.toMatchObject({
        variant: "DataCloneError",
      });
    });
  });

  it("rejects a worker call to an undeclared callback method", async () => {
    const { session } = host(async (key, _args, context) =>
      context.callbacks.invoke(1, key, []),
    );
    const getPrivateKey = vi.fn(() => "secret");
    session.callbacks.register(
      "Signer",
      { sign: async () => "signed", getPrivateKey },
      ["identity", "kind", "sign"],
    );
    await session.ready();
    for (const method of ["getPrivateKey", "toString", "constructor"])
      await expect(session.call(method, [])).rejects.toMatchObject({
        code: "ContractMismatch",
      });
    expect(getPrivateKey).not.toHaveBeenCalled();
    await expect(session.call("sign", [])).resolves.toBe("signed");
  });

  it("contract_mismatch_refused", async () => {
    const [main, worker] = pair();
    new WorkerHost(
      worker,
      1,
      "worker hash",
      async () => {},
      async () => undefined,
    );
    const session = new MainSession(main, 1, "main hash");
    await expect(session.ready()).rejects.toMatchObject({
      code: "ContractMismatch",
    });
  });

  it("storage_busy_second_tab", async () => {
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
    const first = new PoolLocks(provider);
    const second = new PoolLocks(provider);
    await first.open("same-pool");
    await expect(second.open("same-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.closeAll();
  });

  it("uses one OPFS SAH pool lock for all persistent paths", () => {
    const options = (path: string) => ({
      storage: { location: { tag: "Path", inner: [path] }, label: path },
    });
    expect(poolName(options("first.db"), "opfs-directory")).toBe(
      "opfs-directory",
    );
    expect(poolName(options("second.db"), "opfs-directory")).toBe(
      "opfs-directory",
    );
    expect(
      poolName(
        { storage: { location: { tag: "InMemory" } } },
        "opfs-directory",
      ),
    ).toBeUndefined();
  });

  it("shares one pool lock between clients in one worker", async () => {
    let requests = 0;
    const provider: LockProvider = {
      async request(_name, _options, callback) {
        requests++;
        await callback({});
      },
    };
    const locks = new PoolLocks(provider);
    await Promise.all([locks.open("same-pool"), locks.open("same-pool")]);
    expect(requests).toBe(1);
    locks.attachOwner(1, "same-pool");
    locks.attachOwner(2, "same-pool");
    locks.closeOwner(1);
    await locks.open("same-pool");
    locks.closeOwner(2);
    locks.close("same-pool");
    await locks.open("same-pool");
    locks.closeAll();
  });

  it("keeps a shared lock when one in-flight create fails", async () => {
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
    const first = new PoolLocks(provider);
    const second = new PoolLocks(provider);
    await Promise.all([first.open("race"), first.open("race")]);
    first.close("race");
    await expect(second.open("race")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    first.attachOwner(7, "race");
    first.closeOwner(7);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await second.open("race");
    second.close("race");
  });

  it("keeps the lock when an owner ends during another create", async () => {
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
    const tab = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    await tab.open("pool");
    tab.attachOwner(1, "pool");
    await tab.open("pool");
    tab.closeOwner(1);
    await expect(otherTab.open("pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    tab.attachOwner(2, "pool");
    tab.closeOwner(2);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("pool");
    otherTab.close("pool");
  });

  it("cancels an opening pool lock when the worker closes", async () => {
    const locks = new PoolLocks({
      async request() {
        await new Promise<void>(() => {});
      },
    });
    const opening = locks.open("pending");
    locks.closeAll();
    await expect(opening).rejects.toMatchObject({ code: "WorkerTerminated" });
  });

  it("rejects an already aborted call as cancelled", async () => {
    const { session } = host(async () => undefined);
    await session.ready();
    const abort = new AbortController();
    abort.abort();
    await expect(
      session.call("one", [], undefined, abort.signal),
    ).rejects.toMatchObject({ code: "Cancelled" });
  });

  it("cancels a call that waits for the worker handshake", async () => {
    const [main] = pair();
    const session = new MainSession(main, 1, "stalled");
    const abort = new AbortController();
    const added = vi.spyOn(abort.signal, "addEventListener");
    const removed = vi.spyOn(abort.signal, "removeEventListener");
    const call = session.call("waiting", [], undefined, abort.signal);
    await Promise.resolve();
    abort.abort("stop");
    await expect(
      Promise.race([
        call,
        new Promise<never>((_resolve, reject) =>
          setTimeout(() => reject(new Error("call did not settle")), 100),
        ),
      ]),
    ).rejects.toMatchObject({ code: "Cancelled", details: "stop" });
    expect(main.sent.some((message) => message.t === "call")).toBe(false);
    expect(added).toHaveBeenCalledTimes(1);
    expect(removed).toHaveBeenCalledWith("abort", added.mock.calls[0][1]);
  });

  it("leaves no unhandled rejection when a handshake fails", async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown) => unhandled.push(reason);
    process.on("unhandledRejection", onUnhandled);
    try {
      const [main] = pair();
      const session = new MainSession(main, 1, "refused");
      const call = session.call(
        "waiting",
        [],
        undefined,
        new AbortController().signal,
      );
      main.emitRaw({
        t: "refused",
        error: encodeError(bridgeError("contractMismatch")),
      });
      await expect(call).rejects.toMatchObject({ code: "ContractMismatch" });
      await new Promise<void>((resolve) => setTimeout(resolve, 10));
      expect(unhandled).toEqual([]);
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
  });

  it("removes the handshake abort listener when the worker is ready", async () => {
    const [main] = pair();
    const session = new MainSession(main, 1, "late");
    const abort = new AbortController();
    const added = vi.spyOn(abort.signal, "addEventListener");
    const removed = vi.spyOn(abort.signal, "removeEventListener");
    void session.call("waiting", [], undefined, abort.signal);
    await Promise.resolve();
    expect(added).toHaveBeenCalledTimes(1);
    const handshake = added.mock.calls[0][1];
    main.emitRaw({ t: "ready", epoch: 0 });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(removed).toHaveBeenCalledWith("abort", handshake);
    expect(main.sent.some((message) => message.t === "call")).toBe(true);
  });
}
