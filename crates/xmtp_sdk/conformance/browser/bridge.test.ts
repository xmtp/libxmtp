import { describe, expect, it, vi } from "vitest";
import {
  CredentialError_Tags,
  ErrorCategory,
  ListenerError_Tags,
  LogSinkError_Tags,
  PreAuthenticateError_Tags,
  SignerError_Tags,
  XmtpError_Tags,
} from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.js";

import {
  ValueCodec,
  type Layouts,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/codec.js";
import { MainCallbacks } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/callbacks.js";
import {
  RemoteObject,
  endOwner,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/remote-object.js";
import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session.js";
import {
  BRIDGE_ERROR_CODES,
  BridgeError,
  assertCloneable,
  bridgeError,
  decodeError,
  encodeError,
  type WireEndpoint,
  type WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire.js";
import {
  LogWindow,
  WorkerCallbacks,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/callback-stub.js";
import {
  PoolLocks,
  WorkerHost,
  callWithPool,
  poolName,
  type LockProvider,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.js";

class Endpoint implements WireEndpoint {
  peer?: Endpoint;
  readonly sent: WireMessage[] = [];
  readonly transfers: Transferable[][] = [];
  private receive: (message: WireMessage) => void = () => {};
  private exitHandler: () => void = () => {};

  postMessage(message: WireMessage, transfer: Transferable[] = []): void {
    this.sent.push(message);
    this.transfers.push(transfer);
    const copy = structuredClone(message);
    queueMicrotask(() => this.peer?.receive(copy));
  }

  onMessage(handler: (message: WireMessage) => void): void {
    this.receive = handler;
  }
  onExit(handler: () => void): void {
    this.exitHandler = handler;
  }
  emitRaw(message: unknown): void {
    this.receive(message as WireMessage);
  }
  exit(): void {
    this.exitHandler();
    this.peer?.exitHandler();
  }
  terminate?: () => void;
}

function pair(): [Endpoint, Endpoint] {
  const main = new Endpoint();
  const worker = new Endpoint();
  main.peer = worker;
  worker.peer = main;
  return [main, worker];
}

function host(dispatch: ConstructorParameters<typeof WorkerHost>[4]) {
  const [main, worker] = pair();
  const engine = new WorkerHost(worker, 1, "same", async () => {}, dispatch);
  const session = new MainSession(main, 1, "same");
  return { main, worker, engine, session };
}

class TestProxy extends RemoteObject {
  ping(): Promise<unknown> {
    return this.call("ping", []);
  }
  async end(): Promise<void> {
    await this.call("Client.end", []);
    endOwner(this);
  }
}

function stringKeys(value: object): string[] {
  const keys: string[] = [];
  for (let item: object | null = value; item; item = Object.getPrototypeOf(item))
    keys.push(...Object.getOwnPropertyNames(item));
  return keys;
}

describe("browser bridge transport", () => {
  it("exposes only PascalCase error codes", () => {
    const exposed = [
      CredentialError_Tags,
      ListenerError_Tags,
      LogSinkError_Tags,
      PreAuthenticateError_Tags,
      SignerError_Tags,
      XmtpError_Tags,
    ].flatMap((variants) => Object.values(variants));
    for (const code of BRIDGE_ERROR_CODES) {
      const error = bridgeError(code);
      exposed.push(error.code, encodeError(error).code);
    }
    exposed.push(encodeError(new Error("unknown failure")).code);
    for (const code of exposed) {
      expect(code).toMatch(/^[A-Z][A-Za-z0-9]*$/);
    }
  });

  it("uses the generated Unknown category for fallback errors", () => {
    for (const error of [
      Object.assign(new Error("missing detail"), { tag: "XmtpError" }),
      new Error("plain failure"),
      "non-error failure",
    ]) {
      expect(encodeError(error).category).toBe(ErrorCategory.Unknown);
    }
  });

  it("uses the Rust ClientClosed fields for bridge lifecycle errors", () => {
    expect(bridgeError("clientClosed")).toMatchObject({
      variant: "ClientClosed",
      code: "ClientClosed",
      category: 6,
      retryable: false,
      details: [
        {
          code: "ClientClosed",
          category: 6,
          retryable: false,
          message: "client is closed",
        },
      ],
    });
  });

  it("uses the Rust StorageBusy fields for bridge lock errors", () => {
    expect(bridgeError("storageBusy")).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: 2,
      retryable: true,
    });
  });
  it("uses the XmtpError.Lagged variant for bridge stream errors", () => {
    expect(bridgeError("lagged")).toMatchObject({
      variant: "Lagged",
      code: "Lagged",
      category: 9,
      retryable: true,
    });
  });
  it("XmtpError keeps variant and detail fields", () => {
    const error = new BridgeError(
      "StorageBusy",
      "StorageBusy",
      "storage",
      true,
      "busy",
      { pool: "one" },
    );
    const result = decodeError(structuredClone(encodeError(error)));
    expect(result).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: "storage",
      retryable: true,
      details: { pool: "one" },
    });
    class TaggedError extends Error {
      readonly tag = "StorageBusy";
      readonly inner = [
        { code: "StorageBusy", category: 2, retryable: true, pool: "one" },
      ];
    }
    const tagged = decodeError(
      structuredClone(encodeError(new TaggedError("busy"))),
    );
    expect(tagged).toMatchObject({
      variant: "StorageBusy",
      code: "StorageBusy",
      category: 2,
      retryable: true,
      details: [{ pool: "one" }],
    });
  });

  it("object handles return the same worker object", () => {
    const { engine } = host(async () => undefined);
    const layouts: Layouts = { records: {}, enums: {} };
    const object = { marker: 1 };
    const encoder = new ValueCodec(
      layouts,
      "worker",
      "encode",
      undefined,
      engine.registry,
    );
    const decoder = new ValueCodec(
      layouts,
      "worker",
      "decode",
      undefined,
      engine.registry,
    );
    const shape = { kind: "object", name: "Group" } as const;
    const encoded = encoder.convert(shape, object);
    expect(decoder.convert(shape, structuredClone(encoded))).toBe(object);
  });

  it("rolls back sibling handles when a later snapshot throws", () => {
    const { engine } = host(async () => undefined);
    const registry = engine.registry;
    expect(() =>
      registry.scope(() => [
        registry.add({}, "Group"),
        registry.add({}, "Group", undefined, () => {
          throw new Error("snapshot failed");
        }),
      ]),
    ).toThrow("snapshot failed");
    expect(registry.size).toBe(0);
  });

  it("rolls back nested handles when a snapshot throws", () => {
    const { engine } = host(async () => undefined);
    const registry = engine.registry;
    const kept = registry.add({}, "Group");
    expect(() =>
      registry.add({}, "Client", undefined, (owner) => {
        registry.add({}, "Conversations", owner, (nestedOwner) => {
          registry.add({}, "Group", nestedOwner);
          return {};
        });
        throw new Error("snapshot failed");
      }),
    ).toThrow("snapshot failed");
    expect(registry.size).toBe(1);
    expect(registry.release([kept.h])).toEqual([kept.owner]);
    expect(registry.size).toBe(0);
  });

  it("uses a new epoch for each worker and rejects a proxy from another session", async () => {
    const first = host(async () => undefined);
    const second = host(async () => undefined);
    await Promise.all([first.session.ready(), second.session.ready()]);
    expect(first.engine.registry.epoch).not.toBe(second.engine.registry.epoch);
    const proxy = new TestProxy(
      first.session,
      first.engine.registry.add({}, "Group"),
    );
    const encoder = new ValueCodec(
      { records: {}, enums: {} },
      "main",
      "encode",
      second.session,
    );
    expect(() =>
      encoder.convert({ kind: "object", name: "Group" }, proxy),
    ).toThrow("clientClosed");
  });

  it("batches release messages and transfers returned bytes", async () => {
    const { main, worker, session } = host(async () => ({
      bytes: new Uint8Array([1, 2, 3]),
    }));
    await session.ready();
    session.release([11]);
    session.release([12]);
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(main.sent.filter((message) => message.t === "release")).toEqual([
      { t: "release", handles: [11, 12] },
    ]);
    await session.call("bytes", []);
    expect(
      worker.transfers.some((transfer) =>
        transfer.some((item) => item instanceof ArrayBuffer),
      ),
    ).toBe(true);
  });

  it("accepts sets in cloneable wire values", () => {
    expect(() =>
      assertCloneable(new Set([1n, new Uint8Array([2])])),
    ).not.toThrow();
  });

  it("fails pending calls on an unknown wire message", async () => {
    const { main, session } = host(async () => new Promise<unknown>(() => {}));
    await session.ready();
    const pending = session.call("waiting", []);
    await Promise.resolve();
    main.emitRaw({ t: "futureMessage" });
    await expect(pending).rejects.toMatchObject({ code: "ContractMismatch" });
  });

  it("worker_death_settles_pending", async () => {
    const { main, session } = host(async () => new Promise<unknown>(() => {}));
    await session.ready();
    const call = session.call("wait", []);
    await Promise.resolve();
    expect(
      main.sent.some(
        (message) => message.t === "call" && message.key === "wait",
      ),
    ).toBe(true);
    expect(Reflect.get(session, "pending").size).toBe(1);
    main.exit();
    await expect(
      Promise.race([
        call,
        new Promise<never>((_resolve, reject) =>
          setTimeout(
            () => reject(new Error("pending call did not settle")),
            100,
          ),
        ),
      ]),
    ).rejects.toMatchObject({ code: "WorkerTerminated" });
  });

  it("rolls back a pending call when structuredClone throws", async () => {
    const { session } = host(async () => undefined);
    await session.ready();
    await expect(
      session.call("cannot-clone", [() => undefined]),
    ).rejects.toThrow();
    expect(Reflect.get(session, "pending").size).toBe(0);
  });

  it("panic_closes_clients", async () => {
    const { engine, session } = host(
      async () => new Promise<unknown>(() => {}),
    );
    await session.ready();
    const handle = engine.registry.add({}, "Client");
    engine.registry.add({}, "Group", handle.owner);
    const proxy = new TestProxy(session, handle);
    expect(engine.registry.size).toBe(2);
    const call = proxy.ping();
    engine.fatal(new Error("panic"));
    await expect(call).rejects.toMatchObject({ code: "WorkerTerminated" });
    expect(() => proxy.ping()).toThrowError(BridgeError);
    expect(() => proxy.ping()).toThrow("clientClosed");
  });

  it("release_on_end_and_gc", async () => {
    const { engine, session } = host(async () => undefined);
    await session.ready();
    const handle = engine.registry.add({}, "Client");
    engine.registry.add({}, "Group", handle.owner);
    const proxy = new TestProxy(session, handle);
    expect(engine.registry.size).toBe(2);
    await proxy.end();
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(engine.registry.size).toBe(0);
    expect(() => proxy.ping()).toThrow("clientClosed");
  });

  it("keeps the owner release hook off every proxy", async () => {
    const { engine, session } = host(async () => undefined);
    await session.ready();
    const proxy = new TestProxy(session, engine.registry.add({}, "Client"));
    expect("endOwner" in proxy).toBe(false);
    expect(stringKeys(proxy)).not.toContain("endOwner");
  });

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

  // Models the lock manager of one browser worker. An acquired lock stays
  // held until its callback returns or the worker terminates. A request for
  // "xmtp:pending-pool" never gets its lock.
  function workerLockManager(held: Set<string>): {
    provider: LockProvider;
    terminate: () => void;
  } {
    let terminate = () => {};
    const terminated = new Promise<void>((resolve) => {
      terminate = resolve;
    });
    return {
      terminate: () => terminate(),
      provider: {
        async request(name, _options, callback) {
          if (name === "xmtp:pending-pool") return new Promise<void>(() => {});
          if (held.has(name)) return callback(null);
          held.add(name);
          try {
            await Promise.race([callback({}), terminated]);
          } finally {
            held.delete(name);
          }
        },
      },
    };
  }

  function heldPoolLocks(): { held: Set<string>; provider: LockProvider } {
    const held = new Set<string>();
    return {
      held,
      provider: {
        async request(name, _options, callback) {
          if (held.has(name)) return callback(null);
          held.add(name);
          try {
            await callback({});
          } finally {
            held.delete(name);
          }
        },
      },
    };
  }

  async function createUnencodableClient(
    locks: PoolLocks,
    client: { end: () => Promise<void> },
  ): Promise<void> {
    const { engine } = host(async () => undefined);
    const registry = engine.registry;
    await expect(
      callWithPool(
        locks,
        "client-pool",
        true,
        async () => client,
        () =>
          registry.scope(() =>
            registry.add(client, "Client", undefined, () => {
              throw new Error("snapshot failed");
            }),
          ),
      ),
    ).rejects.toThrow("snapshot failed");
    expect(registry.size).toBe(0);
  }

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

  it("keeps the pool lock when a created client that fails to encode cannot end", async () => {
    const { provider } = heldPoolLocks();
    const locks = new PoolLocks(provider);
    const otherTab = new PoolLocks(provider);
    const client = {
      end: async () => {
        throw new Error("close failed");
      },
    };
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await createUnencodableClient(locks, client);
      expect(logged).toHaveBeenCalledWith(
        "client that failed to encode could not close",
        expect.objectContaining({ message: "close failed" }),
      );
    } finally {
      logged.mockRestore();
    }
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await expect(otherTab.open("client-pool")).rejects.toMatchObject({
      code: "StorageBusy",
    });
    locks.closeAll();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
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

  it("log_window_busy_at_4096", () => {
    const [main, worker] = pair();
    const callbacks = new WorkerCallbacks(worker);
    const sink = new LogWindow(callbacks, 1);
    for (let index = 0; index < 4096; index++)
      expect(sink.log(index)).toBe("accepted");
    expect(sink.outstanding).toBe(4096);
    expect(sink.log(4096)).toBe("busy");
    main.exit();
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
    expect(poolName(options("first.db"))).toBe(".opfs-libxmtp-metadata");
    expect(poolName(options("second.db"))).toBe(poolName(options("first.db")));
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
});
