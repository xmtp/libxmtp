import {
  bridgeError,
  decodeError,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
  handoff?: () => void;
}

export class WorkerCallbacks {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  // Called when a host callback starts, with its handle.
  onInvoke?: (cb: number) => void;

  constructor(private readonly endpoint: WireEndpoint) {}

  invoke(
    cb: number,
    method: string,
    args: unknown[],
    handoff?: () => void,
  ): Promise<unknown> {
    this.onInvoke?.(cb);
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, handoff });
      try {
        this.endpoint.postMessage({ t: "callback", id, cb, method, args });
      } catch (error) {
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  receive(message: Extract<WireMessage, { t: "callbackResult" }>): void {
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    if (message.error) pending.reject(decodeError(message.error));
    else pending.resolve(message.value);
  }

  receiveHandoff(id: number): void {
    const pending = this.pending.get(id);
    const handoff = pending?.handoff;
    if (!pending || !handoff) return;
    pending.handoff = undefined;
    handoff();
  }

  drop(cb: number): void {
    try {
      this.endpoint.postMessage({ t: "callbackDrop", cb });
    } catch {
      // The endpoint can close before a callback stub is collected.
    }
  }

  terminate(): void {
    for (const pending of this.pending.values())
      pending.reject(bridgeError("workerTerminated"));
    this.pending.clear();
  }
}
