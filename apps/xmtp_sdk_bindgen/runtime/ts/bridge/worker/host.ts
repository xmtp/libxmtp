import {
  assertCloneable,
  bridgeError,
  encodeError,
  type HandleWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";
import { WorkerCallbacks } from "./callback-stub.js";
import { WorkerRegistry } from "./registry.js";

export interface LockProvider {
  request(
    name: string,
    options: { ifAvailable: true },
    callback: (lock: object | null) => Promise<void>,
  ): Promise<void>;
}

export function browserPoolLocks(): PoolLocks {
  return new PoolLocks({
    request: (name, options, callback) =>
      navigator.locks.request(name, options, (lock) => callback(lock)),
  });
}

export class PoolLocks {
  private readonly releases = new Map<string, () => void>();
  private readonly openings = new Map<string, Promise<void>>();
  private readonly openingRejects = new Map<string, (error: Error) => void>();
  private readonly owners = new Map<number, string>();
  private readonly users = new Map<string, number>();
  private closed = false;

  constructor(private readonly provider: LockProvider) {}

  async open(pool: string): Promise<void> {
    if (this.closed) throw bridgeError("workerTerminated");
    this.users.set(pool, (this.users.get(pool) ?? 0) + 1);
    try {
      if (this.releases.has(pool)) return;
      let opening = this.openings.get(pool);
      if (!opening) {
        opening = this.openNew(pool);
        this.openings.set(pool, opening);
        void opening.finally(() => this.openings.delete(pool)).catch(() => {});
      }
      await opening;
    } catch (error) {
      this.close(pool);
      throw error;
    }
  }

  private async openNew(pool: string): Promise<void> {
    let entered: (() => void) | undefined;
    let rejected: ((error: Error) => void) | undefined;
    const enteredPromise = new Promise<void>((resolve, reject) => {
      entered = resolve;
      rejected = reject;
    });
    this.openingRejects.set(pool, (error) => rejected?.(error));
    void this.provider
      .request(`xmtp:${pool}`, { ifAvailable: true }, async (lock) => {
        if (this.closed) {
          rejected?.(bridgeError("workerTerminated"));
          return;
        }
        if (!lock) {
          rejected?.(bridgeError("storageBusy"));
          return;
        }
        await new Promise<void>((resolve) => {
          this.releases.set(pool, resolve);
          entered?.();
        });
      })
      .catch((error: unknown) =>
        rejected?.(error instanceof Error ? error : new Error(String(error))),
      );
    try {
      await enteredPromise;
    } finally {
      this.openingRejects.delete(pool);
    }
  }

  close(pool: string): void {
    const remaining = (this.users.get(pool) ?? 0) - 1;
    if (remaining > 0) {
      this.users.set(pool, remaining);
      return;
    }
    this.users.delete(pool);
    this.releases.get(pool)?.();
    this.releases.delete(pool);
  }

  attachOwner(owner: number, pool: string): void {
    this.owners.set(owner, pool);
  }

  closeOwner(owner: number): void {
    const pool = this.owners.get(owner);
    this.owners.delete(owner);
    if (pool) this.close(pool);
  }

  closeAll(): void {
    this.closed = true;
    for (const reject of this.openingRejects.values())
      reject(bridgeError("workerTerminated"));
    this.openingRejects.clear();
    for (const release of this.releases.values()) release();
    this.releases.clear();
    this.users.clear();
    this.owners.clear();
  }
}

export function poolName(options: unknown): string | undefined {
  if (
    options === null ||
    typeof options !== "object" ||
    !("storage" in options)
  )
    return undefined;
  const storage = options.storage;
  if (storage === null || typeof storage !== "object") return undefined;
  const location = "location" in storage ? storage.location : undefined;
  if (location !== null && typeof location === "object" && "tag" in location) {
    if (location.tag === "InMemory") return undefined;
    if (location.tag === "Path" || location.tag === "Directory")
      return ".opfs-libxmtp-metadata";
  }
  return ".opfs-libxmtp-metadata";
}

export interface WorkerContext {
  registry: WorkerRegistry;
  callbacks: WorkerCallbacks;
  locks?: PoolLocks;
  signal: AbortSignal;
  target?: object;
  targetHandle?: HandleWire;
}

export type Dispatch = (
  key: string,
  args: unknown[],
  context: WorkerContext,
) => Promise<unknown>;

export const RUST_PANIC_PREFIX = "[Rust panic]";

export class WorkerHost {
  readonly registry: WorkerRegistry;
  readonly callbacks: WorkerCallbacks;
  private readonly active = new Map<number, AbortController>();
  private initialized = false;
  private failed = false;
  private restorePanicLogger?: () => void;
  private restoreWorkerFailures?: () => void;

  constructor(
    private readonly endpoint: WireEndpoint,
    private readonly version: number,
    private readonly hash: string,
    private readonly initialize: () => Promise<void>,
    private readonly dispatch: Dispatch,
    private readonly locks?: PoolLocks,
  ) {
    const random = crypto.getRandomValues(new Uint32Array(2));
    this.registry = new WorkerRegistry(
      (random[0] % 0x200000) * 0x100000000 + random[1],
    );
    this.callbacks = new WorkerCallbacks(endpoint);
    endpoint.onMessage((message) => this.receive(message));
    endpoint.onExit(() => this.fatal(bridgeError("workerTerminated")));
    if (endpoint.close) {
      this.watchWorkerFailures();
      this.watchRustPanics();
    }
  }

  private receive(message: WireMessage): void {
    if (this.failed) return;
    switch (message.t) {
      case "hello":
        void this.hello(message);
        break;
      case "call":
        void this.run(message);
        break;
      case "cancel":
        this.active.get(message.id)?.abort();
        break;
      case "release": {
        void this.releaseHandles(message);
        break;
      }
      case "callbackResult":
        this.callbacks.receive(message);
        break;
      default:
        console.error("unknown bridge message", message);
        this.fatal(bridgeError("contractMismatch", message));
    }
  }

  private async releaseHandles(
    message: Extract<WireMessage, { t: "release" }>,
  ): Promise<void> {
    const emptyOwners = this.registry.release(message.handles);
    const endedOwners = new Set(message.owners ?? []);
    for (const owner of endedOwners) {
      this.registry.closeOwner(owner);
      this.locks?.closeOwner(owner);
    }
    for (const owner of emptyOwners) {
      if (endedOwners.has(owner)) continue;
      const client = this.registry.takeClient(owner);
      if (client) {
        try {
          const end: unknown = Reflect.get(client, "end");
          if (typeof end !== "function")
            throw new TypeError("Client.end is missing");
          await Reflect.apply(end, client, []);
        } catch (error) {
          console.error("collected client could not close", error);
          continue;
        }
      }
      this.locks?.closeOwner(owner);
    }
  }

  private async hello(
    message: Extract<WireMessage, { t: "hello" }>,
  ): Promise<void> {
    if (message.version !== this.version || message.hash !== this.hash) {
      this.endpoint.postMessage({
        t: "refused",
        error: encodeError(bridgeError("contractMismatch")),
      });
      return;
    }
    try {
      await this.initialize();
      this.initialized = true;
      this.endpoint.postMessage({ t: "ready", epoch: this.registry.epoch });
    } catch (error) {
      this.fatal(error);
    }
  }

  private async run(
    message: Extract<WireMessage, { t: "call" }>,
  ): Promise<void> {
    if (!this.initialized || this.failed) return;
    const controller = new AbortController();
    this.active.set(message.id, controller);
    try {
      const value = await this.dispatch(message.key, message.args, {
        registry: this.registry,
        callbacks: this.callbacks,
        locks: this.locks,
        signal: controller.signal,
        target: message.target ? this.registry.get(message.target) : undefined,
        targetHandle: message.target,
      });
      const reply: WireMessage = { t: "return", id: message.id, value };
      assertCloneable(reply);
      this.endpoint.postMessage(reply, transferBuffers(reply));
    } catch (error) {
      if (error instanceof WebAssembly.RuntimeError) {
        this.fatal(error);
        return;
      }
      if (this.isFailed()) return;
      this.endpoint.postMessage({
        t: "error",
        id: message.id,
        error: encodeError(error),
      });
    } finally {
      this.active.delete(message.id);
    }
  }

  fatal(error: unknown): void {
    if (this.failed) return;
    this.failed = true;
    this.initialized = false;
    this.restorePanicLogger?.();
    this.restoreWorkerFailures?.();
    for (const controller of this.active.values()) controller.abort();
    this.active.clear();
    this.callbacks.terminate();
    this.locks?.closeAll();
    try {
      this.endpoint.postMessage({ t: "fatal", error: encodeError(error) });
    } catch {
      // The worker can close the endpoint before the fatal message is sent.
    }
    queueMicrotask(() => {
      if (this.endpoint.close) this.endpoint.close();
      else if (typeof self !== "undefined" && typeof self.close === "function")
        self.close();
    });
  }

  private watchWorkerFailures(): void {
    if (typeof globalThis.addEventListener !== "function") return;
    const onError = (event: Event): void => {
      this.fatal("error" in event ? event.error : event);
    };
    const onRejection = (event: Event): void => {
      this.fatal("reason" in event ? event.reason : event);
    };
    globalThis.addEventListener("error", onError);
    globalThis.addEventListener("unhandledrejection", onRejection);
    this.restoreWorkerFailures = () => {
      globalThis.removeEventListener("error", onError);
      globalThis.removeEventListener("unhandledrejection", onRejection);
    };
  }

  private watchRustPanics(): void {
    // The pinned WASM player has no public panic observer. Its private panic
    // hook logs this prefix before a background task can raise an error event.
    // The conformance test checks the pinned player source for this prefix.
    const previous = console.error;
    const logger = (...args: unknown[]): void => {
      previous(...args);
      if (
        typeof args[0] === "string" &&
        args[0].startsWith(`${RUST_PANIC_PREFIX} `)
      )
        this.fatal(new WebAssembly.RuntimeError(args[0]));
    };
    console.error = logger;
    this.restorePanicLogger = () => {
      if (console.error === logger) console.error = previous;
    };
  }

  private isFailed(): boolean {
    return this.failed;
  }
}

function transferBuffers(value: unknown): ArrayBuffer[] {
  const buffers = new Set<ArrayBuffer>();
  const visit = (part: unknown): void => {
    if (part instanceof Uint8Array) {
      if (part.buffer instanceof ArrayBuffer) buffers.add(part.buffer);
    } else if (part instanceof ArrayBuffer) buffers.add(part);
    else if (Array.isArray(part)) part.forEach(visit);
    else if (part instanceof Map)
      for (const [key, item] of part) {
        visit(key);
        visit(item);
      }
    else if (part instanceof Set) for (const item of part) visit(item);
    else if (part !== null && typeof part === "object")
      Object.values(part).forEach(visit);
  };
  visit(value);
  return [...buffers];
}
