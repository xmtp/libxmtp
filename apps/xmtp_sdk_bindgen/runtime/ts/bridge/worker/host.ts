import {
  abandonedAtEnd,
  assertCloneable,
  bridgeError,
  encodeError,
  type HandleWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";
import { WorkerCallbacks } from "./callback-stub.js";
import { OwnerCalls } from "./owner-calls.js";
import { WorkerRegistry } from "./registry.js";

export interface LockProvider {
  request(
    name: string,
    options: { ifAvailable: true },
    callback: (lock: object | null) => Promise<void>,
  ): Promise<void>;
}

export function browserPoolLocks(retainIdleLocks = false): PoolLocks {
  return new PoolLocks(
    {
      request: (name, options, callback) =>
        navigator.locks.request(name, options, (lock) => callback(lock)),
    },
    retainIdleLocks,
  );
}

export class PoolLocks {
  private readonly releases = new Map<string, () => void>();
  private readonly openings = new Map<string, Promise<void>>();
  private readonly openingRejects = new Map<string, (error: Error) => void>();
  private readonly owners = new Map<number, string>();
  private readonly users = new Map<string, number>();
  private closed = false;

  constructor(
    private readonly provider: LockProvider,
    private readonly retainIdleLocks = false,
  ) {}

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
    if (this.closed) return;
    const remaining = (this.users.get(pool) ?? 0) - 1;
    if (remaining > 0) {
      this.users.set(pool, remaining);
      return;
    }
    this.users.delete(pool);
    if (this.retainIdleLocks) return;
    this.releases.get(pool)?.();
    this.releases.delete(pool);
  }

  attachOwner(owner: number, pool: string): void {
    this.owners.set(owner, pool);
  }

  poolForOwner(owner: number): string | undefined {
    return this.owners.get(owner);
  }

  closeOwner(owner: number): void {
    const pool = this.owners.get(owner);
    this.owners.delete(owner);
    if (pool) this.close(pool);
  }

  /**
   * Stops lock work in a worker that failed. Pending lock requests are
   * rejected, and a later open or close does nothing. Acquired locks stay
   * held, because WASM and SQLite state can still be open in this worker. The
   * browser releases them when the worker terminates.
   */
  abandon(): void {
    this.closed = true;
    for (const reject of this.openingRejects.values())
      reject(bridgeError("workerTerminated"));
    this.openingRejects.clear();
  }

  closeAll(): void {
    this.abandon();
    for (const release of this.releases.values()) release();
    this.releases.clear();
    this.users.clear();
    this.owners.clear();
  }
}

/**
 * Returns the storage lock name for client options, or `undefined` for an
 * in-memory store. Every persistent store uses the one OPFS pool in
 * `directory`, so they share one lock. The generated dispatch passes the
 * directory from the Rust store configuration.
 */
export function poolName(
  options: unknown,
  directory: string,
): string | undefined {
  if (
    options === null ||
    typeof options !== "object" ||
    !("storage" in options)
  )
    return undefined;
  const storage = options.storage;
  if (storage === null || typeof storage !== "object") return undefined;
  const location = "location" in storage ? storage.location : undefined;
  if (
    location !== null &&
    typeof location === "object" &&
    "tag" in location &&
    location.tag === "InMemory"
  )
    return undefined;
  return directory;
}

/**
 * Runs one binding call and encodes its result. When `pool` is set, the call
 * holds that storage pool lock. Only an explicit client or admin creation
 * transfers its lease to the returned owner. Other calls release their lease.
 * When a created owner fails to encode, it is ended before the lock is
 * released, so its database closes first. If the owner
 * cannot end, its database can stay open, so the lock stays held and the call
 * throws `UnendedClientError`. The worker host then ends the worker. The
 * browser releases a held Web Lock when the worker ends. A failed create can
 * also leave the store of the client that Rust built open, and so can a
 * create or build that is cancelled after its store opened. Then
 * `requiresWorkerRestart` also reports failed VFS transitions. It is checked
 * after every failed call before any pool lease is released.
 */
export async function callWithPool(
  locks: PoolLocks | undefined,
  pool: string | undefined,
  createsOwner: boolean,
  call: () => unknown,
  encode: (result: unknown) => unknown,
  requiresWorkerRestart: () => boolean,
  started: () => void = () => {},
  created: (owner: number) => void = () => {},
): Promise<unknown> {
  if (pool) {
    if (!locks) throw new TypeError("storage lock provider missing");
    await locks.open(pool);
  }
  let result: unknown;
  try {
    started();
    result = await call();
  } catch (error) {
    if (requiresWorkerRestart())
      throw new UnendedClientError(
        error,
        new Error("storage requires worker termination"),
      );
    if (pool) locks?.close(pool);
    throw error;
  }
  let encoded: unknown;
  try {
    encoded = encode(result);
  } catch (error) {
    if (createsOwner) {
      const failure = await endUnencodedClient(result);
      if (failure) throw new UnendedClientError(error, failure.error);
    }
    if (pool) locks?.close(pool);
    throw error;
  }
  if (
    createsOwner &&
    encoded !== null &&
    typeof encoded === "object" &&
    "owner" in encoded &&
    typeof encoded.owner === "number"
  ) {
    if (pool) locks?.attachOwner(encoded.owner, pool);
    created(encoded.owner);
  } else if (pool) locks?.close(pool);
  return encoded;
}

/**
 * A created client could not close: it failed to encode and then could not
 * end, or the create failed and left its store open. The caller gets
 * `callError`. The worker then fails with `endError`, because the client's
 * database can still be open and its pool lock stays held.
 */
export class UnendedClientError extends Error {
  constructor(
    readonly callError: unknown,
    readonly endError: unknown,
  ) {
    super("client that failed could not close");
  }
}

// Returns the end error when the client did not end.
async function endUnencodedClient(
  client: unknown,
): Promise<{ error: unknown } | undefined> {
  try {
    const end: unknown =
      client !== null && typeof client === "object"
        ? Reflect.get(client, "end")
        : undefined;
    if (typeof end !== "function") throw new TypeError("Client.end is missing");
    await Reflect.apply(end, client, []);
    return undefined;
  } catch (error) {
    console.error("client that failed to encode could not close", error);
    return { error };
  }
}

export interface WorkerContext {
  registry: WorkerRegistry;
  callbacks: WorkerCallbacks;
  locks?: PoolLocks;
  signal: AbortSignal;
  target?: object;
  targetHandle?: HandleWire;
  started?: () => void;
  // The generated dispatch calls this after the binding call and its result
  // encoding finish. Database work for the call is then complete.
  settled?: () => void;
  createdOwner?: number;
}

/**
 * Rejects a call whose target handle does not fit its operation. A method
 * needs a handle of its own class. A constructor or a function needs no
 * handle. The registry resolved the target only if the handle type is the
 * type that the handle got when it was created, so the handle type is the
 * type of the target object.
 */
export function checkTarget(
  key: string,
  type: string | undefined,
  context: WorkerContext,
): void {
  const actual = context.targetHandle?.type;
  if (actual !== type || (type !== undefined && context.target === undefined))
    throw bridgeError("contractMismatch", { key, target: actual });
}

export type Dispatch = (
  key: string,
  args: unknown[],
  context: WorkerContext,
) => Promise<unknown>;

/** The host callback handles in a call's wire arguments. */
function callbackHandles(value: unknown, found: number[] = []): number[] {
  if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) return found;
  if (Array.isArray(value)) {
    for (const item of value) callbackHandles(item, found);
  } else if (value !== null && typeof value === "object") {
    const cb: unknown = Reflect.get(value, "cb");
    if (
      typeof cb === "number" &&
      typeof Reflect.get(value, "type") === "string"
    )
      found.push(cb);
    else for (const item of Object.values(value)) callbackHandles(item, found);
  }
  return found;
}

export const RUST_PANIC_PREFIX = "[Rust panic]";

export class WorkerHost {
  readonly registry: WorkerRegistry;
  readonly callbacks: WorkerCallbacks;
  private readonly active = new Map<number, AbortController>();
  private readonly ownerCalls = new OwnerCalls();
  // Owners whose root ended through a successful client or admin end call,
  // with the calls that must finish before the owner's storage lock is
  // released.
  private readonly endedOwners = new Map<number, Promise<void>>();
  private initialized = false;
  private revision = 0;
  private idleRevision = -1;
  private idlePreparing = false;
  private releasesPending = 0;
  private failed = false;
  private restorePanicLogger?: () => void;
  private restoreWorkerFailures?: () => void;

  constructor(
    private readonly endpoint: WireEndpoint,
    private readonly version: number,
    private readonly hash: string,
    private readonly initialize: (lifetimeLock?: string) => Promise<void>,
    private readonly dispatch: Dispatch,
    private readonly locks?: PoolLocks,
    private readonly prepareIdle: () => void | Promise<void> = () => {},
  ) {
    const random = crypto.getRandomValues(new Uint32Array(2));
    this.registry = new WorkerRegistry(
      (random[0] % 0x200000) * 0x100000000 + random[1],
    );
    this.callbacks = new WorkerCallbacks(endpoint);
    this.callbacks.onInvoke = (cb) => this.ownerCalls.parked(cb);
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
        this.revision = message.revision ?? this.revision;
        void this.run(message);
        break;
      case "cancel":
        this.active.get(message.id)?.abort();
        break;
      case "release": {
        this.revision = message.revision ?? this.revision;
        this.releasesPending++;
        void this.releaseHandles(message)
          .catch((error: unknown) => this.fatal(error))
          .finally(() => {
            this.releasesPending--;
            this.reportIdle();
          });
        break;
      }
      case "logHandoff":
        this.callbacks.receiveHandoff(message.id);
        break;
      case "callbackResult":
        this.callbacks.receive(message);
        break;
      default:
        console.error("unknown bridge message", message);
        this.fatal(bridgeError("contractMismatch", message));
    }
  }

  // An owner in `message.owners` is closed with all of its handles. The main
  // thread sends one after client or admin end resolved. If that root did
  // not end through an explicit call, it is ended here before its storage
  // lock is released, as for a collected root.
  private async releaseHandles(
    message: Extract<WireMessage, { t: "release" }>,
  ): Promise<void> {
    const emptyOwners = this.registry.release(message.handles);
    const closedOwners = new Set(message.owners ?? []);
    const clients = new Map<number, object | undefined>();
    for (const owner of new Set([...closedOwners, ...emptyOwners]))
      clients.set(owner, this.registry.takeRoot(owner));
    for (const owner of closedOwners) this.registry.closeOwner(owner);
    for (const [owner, client] of clients) {
      const ended = this.endedOwners.get(owner);
      if (ended) {
        this.endedOwners.delete(owner);
        await ended;
        this.locks?.closeOwner(owner);
        this.ownerCalls.forget(owner);
        continue;
      }
      if (client) {
        try {
          const closing = this.ownerCalls.fence(owner);
          await closing.started;
          const end: unknown = Reflect.get(client, "end");
          if (typeof end !== "function")
            throw new TypeError("Client.end is missing");
          await Reflect.apply(end, client, []);
          await closing.drained;
          await closing.done;
        } catch (error) {
          console.error("collected client could not close", error);
          this.fatal(error);
          return;
        }
      }
      this.locks?.closeOwner(owner);
      this.ownerCalls.forget(owner);
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
      await this.initialize(message.lifetimeLock);
      this.initialized = true;
      this.endpoint.postMessage({ t: "ready", epoch: this.registry.epoch });
      this.reportIdle();
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
    let accepted: ReturnType<OwnerCalls["accept"]> | undefined;
    let closing: ReturnType<OwnerCalls["fence"]> | undefined;
    const context: WorkerContext = {
      registry: this.registry,
      callbacks: this.callbacks,
      locks: this.locks,
      signal: controller.signal,
      targetHandle: message.target,
    };
    try {
      const target = message.target
        ? this.registry.get(message.target)
        : undefined;
      const owner = message.target?.owner;
      const endsOwner =
        owner !== undefined &&
        target !== undefined &&
        target === this.registry.root(owner) &&
        (message.key === "Client.end" || message.key === "StorageAdmin.end");
      if (owner !== undefined) {
        if (endsOwner) {
          closing = this.ownerCalls.fence(owner);
          await closing.started;
        } else
          accepted = this.ownerCalls.accept(
            owner,
            abandonedAtEnd(message.key),
            callbackHandles(message.args),
          );
      }
      context.target = target;
      context.started = () => accepted?.start();
      context.settled = () => accepted?.settle();
      const value = await this.dispatch(message.key, message.args, context);
      if (closing) {
        // A call parked on a host callback does not hold up the end reply;
        // its host code may be waiting for this end. The storage lock waits
        // for it when the owner is released.
        await closing.drained;
        this.endedOwners.set(message.target!.owner, closing.done);
      }
      const reply: WireMessage = { t: "return", id: message.id, value };
      assertCloneable(reply);
      this.endpoint.postMessage(reply, transferBuffers(reply));
    } catch (caught) {
      let error = caught;
      if (closing && message.target)
        this.ownerCalls.reopen(message.target.owner);
      if (context.createdOwner !== undefined) {
        const owner = context.createdOwner;
        const root = this.registry.takeRoot(owner);
        this.registry.closeOwner(owner);
        const failure = await endUnencodedClient(root);
        if (failure) error = new UnendedClientError(error, failure.error);
        else this.locks?.closeOwner(owner);
      }
      if (error instanceof WebAssembly.RuntimeError) {
        this.fatal(error);
        return;
      }
      if (this.isFailed()) return;
      const unended = error instanceof UnendedClientError ? error : undefined;
      this.endpoint.postMessage({
        t: "error",
        id: message.id,
        error: encodeError(unended ? unended.callError : error),
        fatal: unended !== undefined,
      });
      if (unended) this.fatal(unended.endError);
    } finally {
      accepted?.finish();
      this.active.delete(message.id);
      this.reportIdle();
    }
  }

  private canReportIdle(): boolean {
    return (
      this.initialized &&
      !this.failed &&
      this.active.size === 0 &&
      this.releasesPending === 0 &&
      this.registry.size === 0 &&
      this.idleRevision !== this.revision
    );
  }

  private reportIdle(): void {
    if (this.idlePreparing || !this.canReportIdle()) return;
    const revision = this.revision;
    this.idlePreparing = true;
    // Preparation can wait for Rust log delivery. Keep it off every operation
    // response path so a log callback can call or end SDK objects.
    void Promise.resolve()
      .then(() => this.prepareIdle())
      .then(() => {
        if (revision !== this.revision || !this.canReportIdle()) return;
        this.idleRevision = revision;
        this.endpoint.postMessage({ t: "idle", revision });
      })
      .catch((error: unknown) => this.fatal(error))
      .finally(() => {
        this.idlePreparing = false;
        this.reportIdle();
      });
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
    // The worker closes itself below, and the main thread terminates it when
    // it gets the fatal message. Termination releases the acquired locks.
    this.locks?.abandon();
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
    // The Browser panic fixture checks delivery through this hook.
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
