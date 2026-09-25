import { ConnectionState } from "../../xmtp_sdk";
import type { Conversation } from "../../xmtp_sdk";
import type { Client } from "../client";
import type { Message } from "../message";

const done = { done: true, value: undefined } as const;

export type StreamCloseReason =
  | { kind: "closed" }
  | { kind: "failed"; error: unknown };

export type StreamOptions = {
  signal?: AbortSignal;
  onClose?: (reason: StreamCloseReason) => void;
  onConnectionStateChange?: (
    previous: ConnectionState | undefined,
    current: ConnectionState,
  ) => void;
};

type ReaderLike<T> = {
  next(options?: { signal: AbortSignal }): Promise<T | undefined>;
  end(): Promise<void>;
  connectionState?(): ConnectionState;
  connectionStateChanged?(previous: ConnectionState): Promise<ConnectionState>;
};

/** Each next request acknowledges the value returned by the prior request. */
export class ReaderStream<T> implements AsyncIterableIterator<T> {
  private readonly reader: Promise<ReaderLike<T> | undefined>;
  private readonly stopped: Promise<undefined>;
  private stop!: () => void;
  private active?: ReaderLike<T>;
  private pending?: AbortController;
  private closed = false;
  private closeReason?: StreamCloseReason;

  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<T>>,
    private readonly owner: Client,
    private readonly options: StreamOptions = {},
  ) {
    this.stopped = new Promise((resolve) => {
      this.stop = () => resolve(undefined);
    });
    const creation = new AbortController();
    this.pending = creation;
    this.reader = Promise.resolve()
      .then(() => open(creation.signal))
      .then(async (reader) => {
        if (this.closed) {
          await reader.end();
          return undefined;
        }
        this.active = reader;
        void this.watchConnection(reader);
        return reader;
      });
    void this.reader.catch((error: unknown) => {
      if (!this.closed) this.fail(error);
    });
    this.options.signal?.addEventListener("abort", () => void this.return(), {
      once: true,
    });
    if (this.options.signal?.aborted) void this.return();
  }

  [Symbol.asyncIterator](): AsyncIterableIterator<T> {
    return this;
  }

  private isClosed(): boolean {
    return this.closed;
  }

  private closedResult(): IteratorResult<T> {
    if (this.closeReason?.kind === "failed") throw this.closeReason.error;
    return done;
  }

  async ready(): Promise<void> {
    await Promise.race([this.reader, this.stopped]);
    if (this.closeReason?.kind === "failed") throw this.closeReason.error;
  }

  private notifyClose(reason: StreamCloseReason): void {
    if (this.closeReason !== undefined) return;
    this.closeReason = reason;
    this.options.onClose?.(reason);
  }

  private fail(error: unknown): void {
    if (this.closed) return;
    this.closed = true;
    this.pending?.abort();
    this.stop();
    try {
      this.notifyClose({ kind: "failed", error });
    } finally {
      void this.active?.end().catch(() => undefined);
    }
  }

  private async watchConnection(reader: ReaderLike<T>): Promise<void> {
    const callback = this.options.onConnectionStateChange;
    if (callback === undefined || reader.connectionState === undefined) return;
    let previous: ConnectionState | undefined;
    const emit = (current: ConnectionState): void => {
      if (this.isClosed() || previous === current) return;
      callback(previous, current);
      previous = current;
    };
    let current = ConnectionState.Connecting;
    emit(current);
    try {
      current = reader.connectionState();
      emit(current);
      while (!this.isClosed() && reader.connectionStateChanged !== undefined) {
        current = await reader.connectionStateChanged(current);
        emit(current);
      }
    } catch {
      // The read path reports terminal errors to the caller and onClose.
    }
  }

  async next(): Promise<IteratorResult<T>> {
    if (this.closed) return this.closedResult();
    try {
      const reader = await Promise.race([this.reader, this.stopped]);
      if (this.isClosed() || reader === undefined) return this.closedResult();
      const read = new AbortController();
      this.pending = read;
      try {
        // Keep the host client alive while this reader is open.
        void this.owner.raw;
        const value = await Promise.race([
          reader.next({ signal: read.signal }),
          this.stopped,
        ]);
        if (this.isClosed()) return this.closedResult();
        if (value === undefined) {
          await this.end();
          return done;
        }
        return { done: false, value };
      } finally {
        if (this.pending === read) this.pending = undefined;
      }
    } catch (error) {
      if (!this.isClosed()) this.fail(error);
      return this.closedResult();
    }
  }

  /** Resolve after the callback; the next read then acknowledges this value. */
  async onValue(callback: (value: T) => void | Promise<void>): Promise<void> {
    try {
      for await (const value of this) await callback(value);
    } catch (error) {
      this.fail(error);
      throw error;
    }
  }

  async return(): Promise<IteratorResult<T>> {
    await this.end();
    return done;
  }

  async end(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.pending?.abort();
    this.stop();
    try {
      this.notifyClose({ kind: "closed" });
    } finally {
      try {
        await this.active?.end();
      } catch {
        // Client shutdown can close the reader first.
      }
    }
  }
}

export class MessageStream extends ReaderStream<Message> {
  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Message>>,
    owner: Client,
    options?: StreamOptions,
  ) {
    super(open, owner, options);
  }
}

export class ConversationStream extends ReaderStream<Conversation> {
  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Conversation>>,
    owner: Client,
    options?: StreamOptions,
  ) {
    super(open, owner, options);
  }
}
