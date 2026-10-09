import {
  abandonedAtEnd,
  bridgeError,
  decodeError,
  encodeError,
  type BridgeErrorCode,
  type HandleWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";
import type { ErrorWire } from "../wire.js";
import { type LogCallbackQueue, MainCallbacks } from "./callbacks.js";
import type { RemoteObject } from "./remote-object.js";

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
  // The owner of a read that is abandoned when that owner ends.
  abandonedOwner?: number;
  eventRead?: boolean;
}

export class MainSession {
  readonly callbacks: MainCallbacks;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private revision = 0;
  private idleRevision = -1;
  private localCalls = 0;
  private readyResolve: (() => void) | undefined;
  private readyReject: ((error: Error) => void) | undefined;
  private readonly readyPromise: Promise<void>;
  private dead = false;
  private stopped = false;
  private stopResolve!: () => void;
  private stopReject!: (error: unknown) => void;
  private readonly stoppedPromise = new Promise<void>((resolve, reject) => {
    this.stopResolve = resolve;
    this.stopReject = reject;
  });
  private epoch = 0;
  private readonly closedOwners = new Set<number>();
  private readonly endedOwners = new Set<number>();
  // The last read on each reader handle. The next read waits for it.
  private readonly readTails = new Map<number, Promise<void>>();
  // Reads that wait for an earlier read. Worker termination fails them.
  private readonly queuedReads = new Set<(error: Error) => void>();
  // Owners whose end is in progress. Reads that arrive for them wait here
  // until the end settles.
  private readonly endingOwners = new Map<
    number,
    { pending: Pending; value: unknown }[]
  >();
  private readonly proxies = new Map<number, Set<WeakRef<RemoteObject>>>();
  private readonly snapshots = new Map<number, Set<number>>();
  private readonly parents = new Map<number, Set<number>>();
  private readonly releases = new Set<number>();
  private releaseScheduled = false;
  private errorDecoder: (error: ErrorWire) => Error = decodeError;
  private heldReads = 0;

  constructor(
    private readonly endpoint: WireEndpoint,
    version: number,
    hash: string,
    private readonly onIdle?: () => void,
    logQueue?: LogCallbackQueue,
  ) {
    void this.stoppedPromise.catch(() => {});
    this.callbacks = new MainCallbacks(
      endpoint,
      () => this.notifyIdle(),
      logQueue,
    );
    this.readyPromise = new Promise<void>((resolve, reject) => {
      this.readyResolve = resolve;
      this.readyReject = reject;
    });
    endpoint.onMessage((message) => this.receive(message));
    endpoint.onExit(() => this.terminate());
    try {
      endpoint.postMessage({ t: "hello", version, hash });
    } catch (error) {
      this.terminate(error);
    }
  }

  ready(): Promise<void> {
    return this.readyPromise;
  }

  get currentEpoch(): number {
    return this.epoch;
  }

  get isTerminated(): boolean {
    return this.dead;
  }

  get terminationComplete(): boolean {
    return this.stopped;
  }

  whenTerminated(): Promise<void> {
    return this.stoppedPromise;
  }

  get isIdle(): boolean {
    return (
      !this.dead &&
      this.localCalls === 0 &&
      !this.callbacks.hasActiveLog &&
      !this.releaseScheduled &&
      this.releases.size === 0 &&
      this.idleRevision === this.revision
    );
  }

  /** A completed value call needs no later worker idle message for migration. */
  get canRetireForMigration(): boolean {
    if (
      this.dead ||
      this.localCalls !== 0 ||
      this.callbacks.hasActiveLog ||
      this.releaseScheduled ||
      this.releases.size !== 0
    )
      return false;
    for (const handle of this.proxies.keys()) {
      const proxy = this.liveProxy(handle);
      if (proxy && !this.endedOwners.has(proxy.handle.owner)) return false;
    }
    return true;
  }

  private notifyIdle(): void {
    if (this.isIdle) this.onIdle?.();
  }

  private postWork(
    message: Extract<WireMessage, { t: "call" | "release" }>,
  ): void {
    const revision = this.revision + 1;
    this.endpoint.postMessage({ ...message, revision });
    this.revision = revision;
  }

  setErrorDecoder(decode: (error: ErrorWire) => Error): void {
    this.errorDecoder = decode;
  }

  error(code: BridgeErrorCode, details?: unknown): Error {
    return this.errorDecoder(encodeError(bridgeError(code, details)));
  }

  proxy(handle: HandleWire): RemoteObject | undefined {
    const value = this.liveProxy(handle.h);
    if (
      value &&
      value.handle.owner === handle.owner &&
      value.handle.epoch === handle.epoch
    )
      return value;
    return undefined;
  }

  remember(proxy: RemoteObject): void {
    const refs =
      this.proxies.get(proxy.handle.h) ?? new Set<WeakRef<RemoteObject>>();
    refs.add(new WeakRef(proxy));
    this.proxies.set(proxy.handle.h, refs);
    const children = new Set<number>();
    const scan = (value: unknown): void => {
      if (value === null || typeof value !== "object") return;
      if (
        "h" in value &&
        typeof value.h === "number" &&
        "owner" in value &&
        typeof value.owner === "number"
      ) {
        children.add(value.h);
        this.parents.set(
          value.h,
          (this.parents.get(value.h) ?? new Set()).add(proxy.handle.h),
        );
        if ("snap" in value) scan(value.snap);
        return;
      }
      if (Array.isArray(value)) value.forEach(scan);
      else Object.values(value).forEach(scan);
    };
    scan(proxy.handle.snap);
    this.snapshots.set(proxy.handle.h, children);
  }

  forget(proxy: RemoteObject): void {
    const refs = this.proxies.get(proxy.handle.h);
    if (!refs) return;
    for (const ref of refs) {
      const value = ref.deref();
      if (!value || value === proxy) refs.delete(ref);
    }
    if (refs.size === 0) this.proxies.delete(proxy.handle.h);
  }

  collected(handle: number): void {
    if (this.liveProxy(handle)) return;
    if (
      [...(this.parents.get(handle) ?? [])].some((parent) =>
        this.liveProxy(parent),
      )
    )
      return;
    this.proxies.delete(handle);
    this.parents.delete(handle);
    this.release([handle]);
    for (const child of this.snapshots.get(handle) ?? []) {
      this.parents.get(child)?.delete(handle);
      this.collected(child);
    }
    this.snapshots.delete(handle);
  }

  private liveProxy(handle: number): RemoteObject | undefined {
    const refs = this.proxies.get(handle);
    if (!refs) return undefined;
    for (const ref of refs) {
      const value = ref.deref();
      if (value) return value;
      refs.delete(ref);
    }
    this.proxies.delete(handle);
    return undefined;
  }

  eventReaderEnding(handle: HandleWire): boolean {
    return this.endingOwners.has(handle.owner);
  }

  eventReaderEnded(handle: HandleWire): boolean {
    return this.endedOwners.has(handle.owner);
  }

  checkHandle(handle: HandleWire): void {
    // A held read decodes a handle from a snapshot that this session already
    // holds. It needs no worker, so an ended owner, a stopped worker, or the
    // epoch that `terminate` advances does not refuse it. A call through the
    // resulting proxy is not a held read, so it still fails with ClientClosed.
    if (this.heldReads > 0) return;
    if (
      this.dead ||
      this.closedOwners.has(handle.owner) ||
      handle.epoch !== this.epoch
    ) {
      throw this.error("clientClosed");
    }
  }

  /** Runs a synchronous read of held snapshot values. */
  readHeld<T>(read: () => T): T {
    this.heldReads++;
    try {
      return read();
    } finally {
      this.heldReads--;
    }
  }

  /**
   * Sends one call. Generated code passes `args` as a function that encodes
   * the arguments. The callbacks that the encoding registers belong to this
   * call until it is posted, so a call that is not posted drops them.
   */
  async call(
    key: string,
    args: unknown[] | (() => unknown[]),
    target?: HandleWire,
    signal?: AbortSignal,
  ): Promise<unknown> {
    this.localCalls++;
    try {
      if (
        (key === "EventReader.next" ||
          key === "EventReader.end" ||
          key === "Client.stopListener") &&
        target &&
        this.eventReaderEnded(target)
      )
        return undefined;
      if (target && abandonedAtEnd(key))
        return await this.sendRead(key, args, target, signal);
      return await this.sendCall(key, args, target, signal);
    } finally {
      this.localCalls--;
      this.notifyIdle();
    }
  }

  /**
   * Posts a reader read only after the previous read on that reader settled
   * on this thread. The worker acknowledges a delivered value when the next
   * read starts, so a later read must not reach the worker while an earlier
   * value could still be abandoned at Client.end. After an abandoned read the
   * owner is closed. Later event reads end normally; other reads fail
   * without being posted.
   */
  private sendRead(
    key: string,
    args: unknown[] | (() => unknown[]),
    target: HandleWire,
    signal?: AbortSignal,
  ): Promise<unknown> {
    const previous = this.readTails.get(target.h) ?? Promise.resolve();
    const read = (async () => {
      await this.readTurn(previous, signal);
      return this.sendCall(key, args, target, signal);
    })();
    // A later read waits for this one and for the earlier read, even when
    // this one was aborted while queued and never posted.
    const tail = Promise.all([
      previous,
      read.then(
        () => undefined,
        () => undefined,
      ),
    ]).then(() => undefined);
    this.readTails.set(target.h, tail);
    void tail.then(() => {
      if (this.readTails.get(target.h) === tail)
        this.readTails.delete(target.h);
    });
    return read;
  }

  /**
   * Waits for the previous read on a reader. A queued read that is aborted,
   * or whose worker ends, fails at once and is never posted.
   */
  private readTurn(
    previous: Promise<void>,
    signal?: AbortSignal,
  ): Promise<void> {
    if (this.dead) return Promise.reject(bridgeError("workerTerminated"));
    if (signal?.aborted)
      return Promise.reject(bridgeError("cancelled", signal.reason));
    return new Promise<void>((resolve, reject) => {
      const stop = () => {
        signal?.removeEventListener("abort", abort);
        this.queuedReads.delete(fail);
      };
      const fail = (error: Error) => {
        stop();
        reject(error);
      };
      const abort = () => fail(bridgeError("cancelled", signal?.reason));
      signal?.addEventListener("abort", abort, { once: true });
      this.queuedReads.add(fail);
      void previous.then(() => {
        stop();
        resolve();
      });
    });
  }

  private async sendCall(
    key: string,
    args: unknown[] | (() => unknown[]),
    target?: HandleWire,
    signal?: AbortSignal,
  ): Promise<unknown> {
    if (
      (key === "EventReader.next" ||
        key === "EventReader.end" ||
        key === "Client.stopListener") &&
      target
    ) {
      if (this.eventReaderEnded(target)) return undefined;
      const ending = this.endingOwners.get(target.owner);
      if (ending) {
        // An event read or stop waits for the owner to close. If close fails,
        // issue that call against the open owner instead.
        return new Promise((resolve, reject) => {
          ending.push({
            pending: {
              eventRead: true,
              resolve: () =>
                resolve(
                  this.eventReaderEnded(target)
                    ? undefined
                    : this.sendCall(key, args, target, signal),
                ),
              reject,
            },
            value: undefined,
          });
        });
      }
    }
    if (target) this.checkHandle(target);
    const { value, registered } =
      typeof args === "function"
        ? this.callbacks.collect(args)
        : { value: args, registered: [] };
    try {
      await this.readyOrAbort(signal);
      if (this.dead) throw bridgeError("workerTerminated");
      if (signal?.aborted) throw bridgeError("cancelled", signal.reason);
    } catch (error) {
      this.callbacks.dropAll(registered);
      throw error;
    }
    const id = this.nextId++;
    return new Promise<unknown>((resolve, reject) => {
      const abort = () => this.endpoint.postMessage({ t: "cancel", id });
      this.pending.set(id, {
        abandonedOwner: abandonedAtEnd(key) ? target?.owner : undefined,
        eventRead: key === "EventReader.next",
        resolve: (value) => {
          signal?.removeEventListener("abort", abort);
          resolve(value);
        },
        reject: (error) => {
          signal?.removeEventListener("abort", abort);
          reject(error);
        },
      });
      signal?.addEventListener("abort", abort, { once: true });
      try {
        this.postWork({ t: "call", id, key, target, args: value });
      } catch (error) {
        this.callbacks.dropAll(registered);
        this.pending.delete(id);
        signal?.removeEventListener("abort", abort);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  // A call can start before the worker handshake. An abort before the worker
  // is ready rejects the call with the same error as an abort after it.
  private readyOrAbort(signal?: AbortSignal): Promise<void> {
    if (!signal) return this.readyPromise;
    if (signal.aborted)
      return Promise.reject(bridgeError("cancelled", signal.reason));
    return new Promise<void>((resolve, reject) => {
      const stop = () => signal.removeEventListener("abort", abort);
      const abort = () => {
        stop();
        reject(bridgeError("cancelled", signal.reason));
      };
      signal.addEventListener("abort", abort);
      void this.readyPromise.then(resolve, reject).finally(stop);
    });
  }

  release(handles: number[]): void {
    if (this.dead) return;
    for (const handle of handles) this.releases.add(handle);
    if (this.releaseScheduled || this.releases.size === 0) return;
    this.releaseScheduled = true;
    queueMicrotask(() => {
      this.releaseScheduled = false;
      if (this.dead || this.releases.size === 0) return;
      const batch = [...this.releases];
      this.releases.clear();
      this.postWork({ t: "release", handles: batch });
    });
  }

  closeOwner(owner: number, handles: number[]): void {
    this.closedOwners.add(owner);
    this.endedOwners.add(owner);
    // Event subscriptions end before Client.end returns, even if their reply
    // is still in transit. A late event reply has no handles to release.
    for (const [id, pending] of this.pending) {
      if (pending.eventRead && pending.abandonedOwner === owner) {
        this.pending.delete(id);
        pending.resolve(undefined);
      }
    }
    // The end succeeded, so reads held for it are abandoned and unacknowledged.
    const held = this.endingOwners.get(owner) ?? [];
    this.endingOwners.delete(owner);
    for (const { pending } of held) this.endRead(pending);
    if (!this.dead) this.postWork({ t: "release", handles, owners: [owner] });
  }

  private endRead(pending: Pending): void {
    if (pending.eventRead) pending.resolve(undefined);
    else pending.reject(this.error("clientClosed"));
  }

  fenceOwner(owner: number): void {
    this.closedOwners.add(owner);
    if (!this.endingOwners.has(owner)) this.endingOwners.set(owner, []);
  }

  unfenceOwner(owner: number): void {
    if (this.dead) return;
    this.closedOwners.delete(owner);
    // The end failed, so the client stays open. A held read reaches the app:
    // the reader acknowledges it on its next read, as for any delivered value.
    const held = this.endingOwners.get(owner) ?? [];
    this.endingOwners.delete(owner);
    for (const { pending, value } of held) pending.resolve(value);
  }

  terminate(cause: unknown = bridgeError("workerTerminated")): void {
    if (this.dead) return;
    this.dead = true;
    this.epoch++;
    const error =
      cause instanceof Error ? cause : bridgeError("workerTerminated");
    this.readyReject?.(error);
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    for (const fail of this.queuedReads) fail(error);
    for (const held of this.endingOwners.values())
      for (const { pending } of held) pending.reject(error);
    this.endingOwners.clear();
    this.proxies.clear();
    this.snapshots.clear();
    this.parents.clear();
    this.releases.clear();
    this.callbacks.close();
    const complete = () => {
      this.stopped = true;
      this.stopResolve();
    };
    try {
      const stopping = this.endpoint.terminate?.();
      if (stopping) void stopping.then(complete, this.stopReject);
      else complete();
    } catch (error) {
      this.stopReject(error);
    }
  }

  private receive(message: WireMessage): void {
    if (this.dead) return;
    switch (message.t) {
      case "ready":
        this.epoch = message.epoch;
        this.readyResolve?.();
        break;
      case "idle":
        this.idleRevision = message.revision;
        this.notifyIdle();
        break;
      case "refused":
        this.terminate(this.errorDecoder(message.error));
        break;
      case "return": {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        const owner = pending?.abandonedOwner;
        const value = message.value !== undefined && message.value !== null;
        const ending =
          owner === undefined ? undefined : this.endingOwners.get(owner);
        if (pending && value && ending) {
          // The owner's end is in progress. Hold the value until the end
          // settles: an ended owner abandons it, and a failed end delivers it.
          ending.push({ pending, value: message.value });
        } else if (
          pending &&
          value &&
          owner !== undefined &&
          this.closedOwners.has(owner)
        ) {
          // The owner ended while this read's value was in transit. The value
          // is not handed to the app, so it stays unacknowledged.
          this.endRead(pending);
        } else pending?.resolve(message.value);
        break;
      }
      case "error": {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        // Fence and terminate before exposing the call error. An immediate
        // retry must use a fresh worker even if the fatal message is delayed.
        if (message.fatal) this.terminate();
        pending?.reject(this.errorDecoder(message.error));
        break;
      }
      case "callback":
        void this.callbacks.receive(message);
        break;
      case "callbackDrop":
        this.callbacks.drop(message.cb);
        break;
      case "fatal":
        this.terminate(bridgeError("workerTerminated", message.error));
        break;
      default:
        console.error("unknown bridge message", message);
        this.terminate(bridgeError("contractMismatch", message));
    }
  }
}
