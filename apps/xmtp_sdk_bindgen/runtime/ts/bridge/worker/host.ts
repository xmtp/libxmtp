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
  private readonly owners = new Map<number, string>();

  constructor(private readonly provider: LockProvider) {}

  async open(pool: string): Promise<boolean> {
    if (this.releases.has(pool)) return false;
    const opening = this.openings.get(pool);
    if (opening) {
      await opening;
      return false;
    }
    const attempt = this.openNew(pool);
    this.openings.set(pool, attempt);
    try {
      await attempt;
      return true;
    } finally {
      this.openings.delete(pool);
    }
  }

  private async openNew(pool: string): Promise<void> {
    let entered: (() => void) | undefined;
    let rejected: ((error: Error) => void) | undefined;
    const enteredPromise = new Promise<void>((resolve, reject) => {
      entered = resolve;
      rejected = reject;
    });
    void this.provider
      .request(`xmtp:${pool}`, { ifAvailable: true }, async (lock) => {
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
    await enteredPromise;
  }

  close(pool: string): void {
    this.releases.get(pool)?.();
    this.releases.delete(pool);
    for (const [owner, name] of this.owners) {
      if (name === pool) this.owners.delete(owner);
    }
  }

  attachOwner(owner: number, pool: string): void {
    this.owners.set(owner, pool);
  }

  closeOwner(owner: number): void {
    const pool = this.owners.get(owner);
    this.owners.delete(owner);
    if (pool && !Array.from(this.owners.values()).includes(pool))
      this.close(pool);
  }

  closeAll(): void {
    for (const pool of this.releases.keys()) this.close(pool);
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
    if (
      (location.tag === "Path" || location.tag === "Directory") &&
      "inner" in location &&
      Array.isArray(location.inner) &&
      typeof location.inner[0] === "string"
    ) {
      return location.inner[0];
    }
  }
  return "label" in storage && typeof storage.label === "string"
    ? storage.label
    : "default";
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

export class WorkerHost {
  readonly registry: WorkerRegistry;
  readonly callbacks: WorkerCallbacks;
  private readonly active = new Map<number, AbortController>();
  private initialized = false;
  private failed = false;

  constructor(
    private readonly endpoint: WireEndpoint,
    private readonly version: number,
    private readonly hash: string,
    private readonly initialize: () => Promise<void>,
    private readonly dispatch: Dispatch,
    private readonly locks?: PoolLocks,
  ) {
    this.registry = new WorkerRegistry(1);
    this.callbacks = new WorkerCallbacks(endpoint);
    endpoint.onMessage((message) => this.receive(message));
    endpoint.onExit(() => this.fatal(bridgeError("workerTerminated")));
  }

  private receive(message: WireMessage): void {
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
      case "release":
        this.registry.release(message.handles);
        for (const owner of message.owners ?? []) {
          this.registry.closeOwner(owner);
          this.locks?.closeOwner(owner);
        }
        break;
      case "callbackResult":
        this.callbacks.receive(message);
        break;
      default:
        break;
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
      this.endpoint.postMessage(reply);
    } catch (error) {
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
    for (const controller of this.active.values()) controller.abort();
    this.active.clear();
    this.callbacks.terminate();
    this.locks?.closeAll();
    try {
      this.endpoint.postMessage({ t: "fatal", error: encodeError(error) });
    } catch {
      // The worker can close the endpoint before the fatal message is sent.
    }
  }
}
