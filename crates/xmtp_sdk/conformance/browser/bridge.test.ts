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
  type LockProvider,
} from "../../../../apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.js";

class Endpoint implements WireEndpoint {
  peer?: Endpoint;
  private receive: (message: WireMessage) => void = () => {};
  private exitHandler: () => void = () => {};

  postMessage(message: WireMessage): void {
    const copy = structuredClone(message);
    queueMicrotask(() => this.peer?.receive(copy));
  }

  onMessage(handler: (message: WireMessage) => void): void {
    this.receive = handler;
  }
  onExit(handler: () => void): void {
    this.exitHandler = handler;
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

  it("worker_death_settles_pending", async () => {
    const { main, session } = host(async () => new Promise<unknown>(() => {}));
    await session.ready();
    const call = session.call("wait", []);
    main.exit();
    await expect(call).rejects.toMatchObject({ code: "workerTerminated" });
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

  it("shares one pool lock between clients in one worker", async () => {
    let requests = 0;
    const provider: LockProvider = {
      async request(_name, _options, callback) {
        requests++;
        await callback({});
      },
    };
    const locks = new PoolLocks(provider);
    const [first, second] = await Promise.all([
      locks.open("same-pool"),
      locks.open("same-pool"),
    ]);
    expect([first, second]).toEqual([true, false]);
    expect(requests).toBe(1);
    locks.attachOwner(1, "same-pool");
    locks.attachOwner(2, "same-pool");
    locks.closeOwner(1);
    expect(await locks.open("same-pool")).toBe(false);
    locks.closeOwner(2);
    expect(await locks.open("same-pool")).toBe(true);
    locks.closeAll();
  });
});
