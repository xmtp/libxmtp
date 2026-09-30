import {
  bridgeError,
  decodeError,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
}

export class WorkerCallbacks {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  // Called when a host callback starts, with its handle.
  onInvoke?: (cb: number) => void;

  constructor(private readonly endpoint: WireEndpoint) {}

  invoke(cb: number, method: string, args: unknown[]): Promise<unknown> {
    this.onInvoke?.(cb);
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
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

export class LogWindow {
  private unacknowledged = 0;
  private readonly limit = 4096;
  private queued: unknown[] = [];
  private scheduled = false;

  constructor(
    private readonly callbacks: WorkerCallbacks,
    private readonly cb: number,
  ) {}

  log(record: unknown): "accepted" | "busy" {
    if (this.unacknowledged >= this.limit) return "busy";
    this.unacknowledged++;
    this.queued.push(record);
    if (!this.scheduled) {
      this.scheduled = true;
      queueMicrotask(() => this.flush());
    }
    return "accepted";
  }

  private flush(): void {
    this.scheduled = false;
    const batch = this.queued;
    this.queued = [];
    void this.callbacks.invoke(this.cb, "logBatch", [batch]).then(
      () => {
        this.unacknowledged -= batch.length;
      },
      () => {
        this.unacknowledged -= batch.length;
      },
    );
  }

  get outstanding(): number {
    return this.unacknowledged;
  }
}
