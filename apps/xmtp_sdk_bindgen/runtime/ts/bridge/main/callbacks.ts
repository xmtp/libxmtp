import {
  encodeError,
  type CallbackWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";

export type CallbackTarget = Partial<
  Record<string, (...args: unknown[]) => unknown>
>;

export class MainCallbacks {
  private readonly targets = new Map<number, CallbackTarget>();
  private nextId = 1;

  constructor(private readonly endpoint: WireEndpoint) {}

  register(type: string, target: CallbackTarget): CallbackWire {
    const cb = this.nextId++;
    if (type === "LogSink") {
      this.targets.set(cb, {
        logBatch: async (records: unknown) => {
          if (!Array.isArray(records)) throw new TypeError("invalid log batch");
          for (const record of records) await target.log?.(record);
        },
      });
    } else {
      this.targets.set(cb, target);
    }
    return { cb, type };
  }

  drop(cb: number): void {
    this.targets.delete(cb);
  }

  clear(): void {
    this.targets.clear();
  }

  async receive(
    message: Extract<WireMessage, { t: "callback" }>,
  ): Promise<void> {
    const target = this.targets.get(message.cb);
    try {
      if (!target) throw new Error("callback was released");
      const method = target[message.method];
      if (!method) throw new Error(`unknown callback method ${message.method}`);
      const value = await method(...message.args);
      this.endpoint.postMessage({ t: "callbackResult", id: message.id, value });
    } catch (error) {
      this.endpoint.postMessage({
        t: "callbackResult",
        id: message.id,
        error: encodeError(error),
      });
    }
  }
}
