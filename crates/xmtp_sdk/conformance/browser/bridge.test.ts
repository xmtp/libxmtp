import { describe, expect, it } from "vitest";

import {
  ValueCodec,
  type Layouts,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/codec.js";
import { MainCallbacks } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/callbacks.js";
import { RemoteObject } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/remote-object.js";
import { MainSession } from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/main/session.js";
import {
  BridgeError,
  assertCloneable,
  bridgeError,
  decodeError,
  encodeError,
  type WireEndpoint,
  type WireMessage,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/wire.js";
import {
  BoundedListener,
  LogWindow,
  WorkerCallbacks,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/callback-stub.js";
import {
  PoolLocks,
  WorkerHost,
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
}

describe("browser bridge transport", () => {
  it("uses the Rust StorageBusy fields for bridge lock errors", () => {
    expect(bridgeError("storageBusy")).toMatchObject({
      variant: "StorageBusy",
      code: "storageBusy",
      category: 2,
      retryable: true,
    });
  });
  it("XmtpError keeps variant and detail fields", () => {
    const error = new BridgeError(
      "StorageBusy",
      "storageBusy",
      "storage",
      true,
      "busy",
      { pool: "one" },
    );
    const result = decodeError(structuredClone(encodeError(error)));
    expect(result).toMatchObject({
      variant: "StorageBusy",
      code: "storageBusy",
      category: "storage",
      retryable: true,
      details: { pool: "one" },
    });
    class TaggedError extends Error {
      readonly tag = "StorageBusy";
      readonly inner = [
        { code: "storageBusy", category: 2, retryable: true, pool: "one" },
      ];
    }
    const tagged = decodeError(
      structuredClone(encodeError(new TaggedError("busy"))),
    );
    expect(tagged).toMatchObject({
      variant: "StorageBusy",
      code: "storageBusy",
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
    await expect(pending).rejects.toMatchObject({ code: "contractMismatch" });
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
    ).rejects.toMatchObject({ code: "workerTerminated" });
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
    await expect(call).rejects.toMatchObject({ code: "workerTerminated" });
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
    proxy.endOwner();
    await new Promise<void>((resolve) => queueMicrotask(resolve));
    expect(engine.registry.size).toBe(0);
    expect(() => proxy.ping()).toThrow("clientClosed");
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
      code: "storageBusy",
    });
    client.release();
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(engine.registry.size).toBe(0);
    await otherTab.open("client-pool");
    otherTab.close("client-pool");
  });

  it("reentrant_signer_completes", async () => {
    const { session, engine } = host(async (key, _args, context) => {
      if (key === "inner") return "inner result";
      return context.callbacks.invoke(1, "sign", []);
    });
    session.callbacks.register("Signer", {
      sign: async () => session.call("inner", []),
    });
    await session.ready();
    await expect(session.call("outer", [])).resolves.toBe("inner result");
    expect(engine.registry.size).toBe(0);
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
      code: "contractMismatch",
    });
  });

  it("listener_bound_1023_then_lagged", async () => {
    const [main, worker] = pair();
    const mainCallbacks = new MainCallbacks(main);
    const workerCallbacks = new WorkerCallbacks(worker);
    main.onMessage((message) => {
      if (message.t === "callback") void mainCallbacks.receive(message);
    });
    worker.onMessage((message) => {
      if (message.t === "callbackResult") workerCallbacks.receive(message);
    });
    let finish: (() => void) | undefined;
    const first = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const lagged: number[] = [];
    const cb = mainCallbacks.register("EventListener", {
      onEvent: async () => first,
      onLagged: (count) => {
        if (typeof count === "number") lagged.push(count);
      },
    });
    const listener = new BoundedListener(workerCallbacks, cb.cb);
    for (let index = 0; index < 1030; index++) listener.push(index);
    expect(listener.queued).toBe(1023);
    finish?.();
    for (let index = 0; index < 10000 && lagged.length === 0; index++)
      await Promise.resolve();
    expect(lagged).toEqual([6]);
  });

  it("continues listener drain after a callback fails", async () => {
    const [main, worker] = pair();
    const mainCallbacks = new MainCallbacks(main);
    const workerCallbacks = new WorkerCallbacks(worker);
    main.onMessage((message) => {
      if (message.t === "callback") void mainCallbacks.receive(message);
    });
    worker.onMessage((message) => {
      if (message.t === "callbackResult") workerCallbacks.receive(message);
    });
    const received: number[] = [];
    const callback = mainCallbacks.register("EventListener", {
      onEvent: (event) => {
        if (typeof event !== "number")
          throw new TypeError("event is not a number");
        received.push(event);
        if (event === 1) throw new Error("first event failed");
      },
    });
    const listener = new BoundedListener(workerCallbacks, callback.cb);
    listener.push(1);
    listener.push(2);
    for (let index = 0; index < 1000 && received.length < 2; index++)
      await Promise.resolve();
    expect(received).toEqual([1, 2]);
    expect(listener.queued).toBe(0);
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
      code: "storageBusy",
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
      code: "storageBusy",
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
      code: "storageBusy",
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
    await expect(opening).rejects.toMatchObject({ code: "workerTerminated" });
  });

  it("reports a gap before later events under constant flow", async () => {
    const [main, worker] = pair();
    const mainCallbacks = new MainCallbacks(main);
    const workerCallbacks = new WorkerCallbacks(worker);
    main.onMessage((message) => {
      if (message.t === "callback") void mainCallbacks.receive(message);
    });
    worker.onMessage((message) => {
      if (message.t === "callbackResult") workerCallbacks.receive(message);
    });
    const order: string[] = [];
    const cb = mainCallbacks.register("EventListener", {
      onEvent: (event) => {
        order.push(`event:${event}`);
      },
      onLagged: (count) => {
        order.push(`lagged:${count}`);
      },
    });
    const listener = new BoundedListener(workerCallbacks, cb.cb);
    for (let index = 0; index < 1030; index++) listener.push(index);
    for (let index = 0; index < 100; index++) {
      listener.push(2000 + index);
      await Promise.resolve();
    }
    for (
      let index = 0;
      index < 10000 && !order.some((item) => item.startsWith("lagged:"));
      index++
    )
      await Promise.resolve();
    const gap = order.findIndex((item) => item.startsWith("lagged:"));
    expect(gap).toBeGreaterThanOrEqual(0);
    expect(
      order.slice(0, gap).every((item) => !/^event:2\d{3}$/.test(item)),
    ).toBe(true);
  });

  it("rejects an already aborted call as cancelled", async () => {
    const { session } = host(async () => undefined);
    await session.ready();
    const abort = new AbortController();
    abort.abort();
    await expect(
      session.call("one", [], undefined, abort.signal),
    ).rejects.toMatchObject({ code: "cancelled" });
  });
});
