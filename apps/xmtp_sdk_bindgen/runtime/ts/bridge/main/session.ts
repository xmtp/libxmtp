import {
  bridgeError,
  decodeError,
  type HandleWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";
import type { ErrorWire } from "../wire.js";
import { MainCallbacks } from "./callbacks.js";
import type { RemoteObject } from "./remote-object.js";

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
}

export class MainSession {
  readonly callbacks: MainCallbacks;
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private readyResolve: (() => void) | undefined;
  private readyReject: ((error: Error) => void) | undefined;
  private readonly readyPromise: Promise<void>;
  private dead = false;
  private epoch = 0;
  private readonly closedOwners = new Set<number>();
  private readonly proxies = new Map<number, Set<WeakRef<RemoteObject>>>();
  private readonly snapshots = new Map<number, Set<number>>();
  private readonly parents = new Map<number, Set<number>>();
  private readonly releases = new Set<number>();
  private releaseScheduled = false;
  private errorDecoder: (error: ErrorWire) => Error = decodeError;

  constructor(
    private readonly endpoint: WireEndpoint,
    version: number,
    hash: string,
  ) {
    this.callbacks = new MainCallbacks(endpoint);
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

  setErrorDecoder(decode: (error: ErrorWire) => Error): void {
    this.errorDecoder = decode;
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

  checkHandle(handle: HandleWire): void {
    if (
      this.dead ||
      this.closedOwners.has(handle.owner) ||
      handle.epoch !== this.epoch
    ) {
      throw bridgeError("clientClosed");
    }
  }

  async call(
    key: string,
    args: unknown[],
    target?: HandleWire,
    signal?: AbortSignal,
  ): Promise<unknown> {
    if (target) this.checkHandle(target);
    await this.readyPromise;
    if (this.dead) throw bridgeError("workerTerminated");
    if (signal?.aborted) throw bridgeError("cancelled", signal.reason);
    const id = this.nextId++;
    return new Promise<unknown>((resolve, reject) => {
      const abort = () => this.endpoint.postMessage({ t: "cancel", id });
      this.pending.set(id, {
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
        this.endpoint.postMessage({ t: "call", id, key, target, args });
      } catch (error) {
        this.pending.delete(id);
        signal?.removeEventListener("abort", abort);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
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
      this.endpoint.postMessage({ t: "release", handles: batch });
    });
  }

  closeOwner(owner: number, handles: number[]): void {
    this.closedOwners.add(owner);
    if (!this.dead)
      this.endpoint.postMessage({ t: "release", handles, owners: [owner] });
  }

  fenceOwner(owner: number): void {
    this.closedOwners.add(owner);
  }

  unfenceOwner(owner: number): void {
    if (!this.dead) this.closedOwners.delete(owner);
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
    this.proxies.clear();
    this.snapshots.clear();
    this.parents.clear();
    this.releases.clear();
    this.callbacks.clear();
  }

  private receive(message: WireMessage): void {
    switch (message.t) {
      case "ready":
        this.epoch = message.epoch;
        this.readyResolve?.();
        break;
      case "refused":
        this.terminate(this.errorDecoder(message.error));
        break;
      case "return":
        this.pending.get(message.id)?.resolve(message.value);
        this.pending.delete(message.id);
        break;
      case "error":
        this.pending.get(message.id)?.reject(this.errorDecoder(message.error));
        this.pending.delete(message.id);
        break;
      case "callback":
        void this.callbacks.receive(message);
        break;
      case "callbackDrop":
        this.callbacks.drop(message.cb);
        break;
      case "fatal":
        this.terminate(bridgeError("workerTerminated", message.error));
        this.endpoint.terminate?.();
        break;
      default:
        console.error("unknown bridge message", message);
        this.terminate(bridgeError("contractMismatch", message));
    }
  }
}
