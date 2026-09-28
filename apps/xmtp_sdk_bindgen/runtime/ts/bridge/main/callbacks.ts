import {
  bridgeError,
  encodeError,
  type CallbackWire,
  type WireEndpoint,
  type WireMessage,
} from "../wire.js";

export type CallbackTarget = Partial<
  Record<string, (...args: unknown[]) => unknown>
>;

interface Registered {
  target: CallbackTarget;
  // The method names that the worker can call. They come from the generated
  // callback interface of the registered type.
  methods: ReadonlySet<string>;
}

export class MainCallbacks {
  private readonly targets = new Map<number, Registered>();
  private nextId = 1;

  constructor(private readonly endpoint: WireEndpoint) {}

  /**
   * Registers a callback object. `methods` is the method list of the
   * generated `type` callback interface. The worker can call only these
   * methods.
   */
  register(
    type: string,
    target: CallbackTarget,
    methods: readonly string[],
  ): CallbackWire {
    const cb = this.nextId++;
    if (type === "LogSink") {
      if (!methods.includes("log"))
        throw new TypeError("LogSink has no generated log method");
      // The worker sends LogSink.log records in batches.
      this.targets.set(cb, {
        methods: new Set(["logBatch"]),
        target: {
          logBatch: async (records: unknown) => {
            if (!Array.isArray(records))
              throw new TypeError("invalid log batch");
            for (const record of records) await target.log?.(record);
          },
        },
      });
    } else {
      this.targets.set(cb, { target, methods: new Set(methods) });
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
    const registered = this.targets.get(message.cb);
    try {
      if (!registered) throw new Error("callback was released");
      const method = registered.methods.has(message.method)
        ? registered.target[message.method]
        : undefined;
      if (typeof method !== "function")
        throw bridgeError("contractMismatch", { method: message.method });
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
