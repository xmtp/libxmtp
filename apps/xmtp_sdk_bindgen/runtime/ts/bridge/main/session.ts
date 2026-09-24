import {
  bridgeError,
  decodeError,
  type HandleWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";
import { MainCallbacks } from "./callbacks.js";

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

  checkHandle(handle: HandleWire): void {
    if (
      this.dead ||
      this.closedOwners.has(handle.owner) ||
      handle.epoch !== this.epoch
    ) {
      throw bridgeError("clientClosed");
    }
  }

  async call<T = unknown>(
    key: string,
    args: unknown[],
    target?: HandleWire,
    signal?: AbortSignal,
  ): Promise<T> {
    await this.readyPromise;
    if (this.dead) throw bridgeError("workerTerminated");
    if (target) this.checkHandle(target);
    if (signal?.aborted) throw bridgeError("callbackFailed", signal.reason);
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const abort = () => this.endpoint.postMessage({ t: "cancel", id });
      this.pending.set(id, {
        resolve: (value) => {
          signal?.removeEventListener("abort", abort);
          resolve(value as T);
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
    if (!this.dead && handles.length > 0) {
      this.endpoint.postMessage({ t: "release", handles });
    }
  }

  closeOwner(owner: number, handles: number[]): void {
    this.closedOwners.add(owner);
    if (!this.dead)
      this.endpoint.postMessage({ t: "release", handles, owners: [owner] });
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
    this.callbacks.clear();
  }

  private receive(message: WireMessage): void {
    switch (message.t) {
      case "ready":
        this.epoch = message.epoch;
        this.readyResolve?.();
        break;
      case "refused":
        this.terminate(decodeError(message.error));
        break;
      case "return":
        this.pending.get(message.id)?.resolve(message.value);
        this.pending.delete(message.id);
        break;
      case "error":
        this.pending.get(message.id)?.reject(decodeError(message.error));
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
        break;
      default:
        break;
    }
  }
}
