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

  constructor(private readonly endpoint: WireEndpoint) {}

  invoke(cb: number, method: string, args: unknown[]): Promise<unknown> {
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

export class BoundedListener {
  private readonly limit = 1023;
  private readonly queue: unknown[] = [];
  private running = false;
  private dropped = 0;

  constructor(
    private readonly callbacks: WorkerCallbacks,
    private readonly cb: number,
  ) {}

  push(event: unknown): void {
    if (this.queue.length === this.limit) {
      this.dropped++;
      return;
    }
    this.queue.push(event);
    if (!this.running) void this.drain();
  }

  private async drain(): Promise<void> {
    this.running = true;
    try {
      while (this.queue.length > 0) {
        const event = this.queue.shift();
        await this.callbacks.invoke(this.cb, "onEvent", [event]);
      }
      if (this.dropped > 0) {
        const count = this.dropped;
        this.dropped = 0;
        await this.callbacks.invoke(this.cb, "onLagged", [count]);
      }
    } finally {
      this.running = false;
      if (this.queue.length > 0) void this.drain();
    }
  }

  get queued(): number {
    return this.queue.length;
  }
}
