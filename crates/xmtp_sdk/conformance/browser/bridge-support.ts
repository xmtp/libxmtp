import { expect } from "vitest";

import {
  RemoteObject,
  endOwner,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/remote-object.js";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session.js";
import {
  type WireEndpoint,
  type WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire.js";
import {
  type PoolLocks,
  WorkerHost,
  callWithPool,
  type LockProvider,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/worker/host.js";

export class Endpoint implements WireEndpoint {
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

export function pair(): [Endpoint, Endpoint] {
  const main = new Endpoint();
  const worker = new Endpoint();
  main.peer = worker;
  worker.peer = main;
  return [main, worker];
}

export function host(dispatch: ConstructorParameters<typeof WorkerHost>[4]) {
  const [main, worker] = pair();
  const engine = new WorkerHost(worker, 1, "same", async () => {}, dispatch);
  const session = new MainSession(main, 1, "same");
  return { main, worker, engine, session };
}

export class TestProxy extends RemoteObject {
  ping(): Promise<unknown> {
    return this.call("ping", []);
  }
  async end(): Promise<void> {
    await this.call("Client.end", []);
    endOwner(this);
  }
}

export async function withoutUnhandledRejections(
  run: () => Promise<void>,
): Promise<void> {
  const unhandled: unknown[] = [];
  const onUnhandled = (reason: unknown) => unhandled.push(reason);
  process.on("unhandledRejection", onUnhandled);
  try {
    await run();
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    expect(unhandled).toEqual([]);
  } finally {
    process.off("unhandledRejection", onUnhandled);
  }
}

export function stringKeys(value: object): string[] {
  const keys: string[] = [];
  for (
    let item: object | null = value;
    item;
    item = Object.getPrototypeOf(item)
  )
    keys.push(...Object.getOwnPropertyNames(item));
  return keys;
}

// Models the lock manager of one browser worker. An acquired lock stays
// held until its callback returns or the worker terminates. A request for
// "xmtp:pending-pool" never gets its lock.
export function workerLockManager(held: Set<string>): {
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

export function heldPoolLocks(): { held: Set<string>; provider: LockProvider } {
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

export async function createUnencodableClient(
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
      () => false,
    ),
  ).rejects.toThrow("snapshot failed");
  expect(registry.size).toBe(0);
}
