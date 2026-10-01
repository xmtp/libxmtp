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
  type: string;
  target: CallbackTarget;
  // The method names that the worker can call. They come from the generated
  // callback interface of the registered type.
  methods: ReadonlySet<string>;
}

export class MainCallbacks {
  private readonly targets = new Map<number, Registered>();
  private nextId = 1;
  private logSink: number | undefined;
  private activeLogs = 0;
  private logUpdate: { staged?: number } | undefined;
  // The ids registered by the `collect` call that is running.
  private scope: number[] | undefined;
  private closed = false;

  constructor(
    private readonly endpoint: WireEndpoint,
    private readonly onLogFinished?: () => void,
  ) {}

  get hasActiveLog(): boolean {
    return this.activeLogs !== 0;
  }

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
      if (this.logUpdate) this.logUpdate.staged = cb;
    }
    this.targets.set(cb, { type, target, methods: new Set(methods) });
    this.scope?.push(cb);
    return { cb, type };
  }

  /**
   * Runs `encode` and returns the callback ids that it registered. The
   * caller drops them if the encoded value is not sent. If `encode` throws,
   * this drops them.
   */
  collect<T>(encode: () => T): { value: T; registered: readonly number[] } {
    const outer = this.scope;
    const registered: number[] = [];
    this.scope = registered;
    try {
      return { value: encode(), registered };
    } catch (error) {
      this.dropAll(registered);
      throw error;
    } finally {
      this.scope = outer;
    }
  }

  /** Commit callback replacement only after the worker accepts the setter. */
  async updateLogSink(update: () => Promise<void>): Promise<void> {
    if (this.logUpdate) throw new Error("log sink update is already active");
    const pending: { staged?: number } = {};
    this.logUpdate = pending;
    try {
      await update();
      this.clearLogSink();
      this.logSink = pending.staged;
    } catch (error) {
      if (pending.staged !== undefined) this.targets.delete(pending.staged);
      throw error;
    } finally {
      this.logUpdate = undefined;
    }
  }

  clearLogSink(): void {
    if (this.logSink !== undefined) this.targets.delete(this.logSink);
    this.logSink = undefined;
  }

  drop(cb: number): void {
    this.targets.delete(cb);
  }

  dropAll(cbs: readonly number[]): void {
    for (const cb of cbs) this.targets.delete(cb);
  }

  /** Drops every callback. Replies to calls that are still running go nowhere. */
  close(): void {
    this.closed = true;
    this.targets.clear();
  }

  /**
   * Runs one worker callback and sends its result. Delivery is best-effort:
   * this never rejects, because the session does not wait for it.
   */
  async receive(
    message: Extract<WireMessage, { t: "callback" }>,
  ): Promise<void> {
    const registered = this.targets.get(message.cb);
    let activeLog = false;
    try {
      let value: unknown;
      try {
        if (!registered) throw new Error("callback was released");
        const method = registered.methods.has(message.method)
          ? registered.target[message.method]
          : undefined;
        if (typeof method !== "function")
          throw bridgeError("contractMismatch", { method: message.method });
        // The receipt and app call have no intervening await. A delayed
        // receipt holds queue credit longer; it cannot release it early.
        if (registered.type === "LogSink" && message.method === "log") {
          this.activeLogs++;
          activeLog = true;
          this.post({ t: "logHandoff", id: message.id });
        }
        value = await method(...message.args);
      } catch (error) {
        this.replyError(
          message.id,
          activeLog ? new Error("log callback failed") : error,
        );
        return;
      }
      try {
        this.post({ t: "callbackResult", id: message.id, value });
      } catch (error) {
        if (isDataCloneError(error)) this.replyError(message.id, error);
      }
    } finally {
      if (activeLog) {
        this.activeLogs--;
        this.onLogFinished?.();
      }
    }
  }

  private replyError(id: number, error: unknown): void {
    try {
      this.post({ t: "callbackResult", id, error: encodeError(error) });
    } catch (sendError) {
      // The error details cannot be cloned. The worker still gets an error.
      if (!isDataCloneError(sendError)) return;
      try {
        this.post({ t: "callbackResult", id, error: encodeError(sendError) });
      } catch {
        // The endpoint closed. The worker is gone, so nobody waits.
      }
    }
  }

  private post(message: WireMessage): void {
    if (this.closed) return;
    this.endpoint.postMessage(message);
  }
}

function isDataCloneError(error: unknown): boolean {
  return error instanceof Error && error.name === "DataCloneError";
}
