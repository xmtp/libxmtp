import { ConnectionState } from "../../xmtp_sdk";
const done = { done: true, value: undefined } as const;

function reportCallbackError(error: unknown): void {
  console.error("XMTP stream close callback failed", error);
}

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

export type ReaderLike<T> = {
  next(options?: { signal: AbortSignal }): Promise<T | undefined>;
  end(): Promise<void>;
  connectionState?(): Promise<ConnectionState>;
  connectionStateChanged?(previous: ConnectionState): Promise<ConnectionState>;
};

/** Each next request acknowledges the value returned by the prior request. */
export class ReaderStream<T> implements AsyncIterableIterator<T> {
  readonly #reader: Promise<ReaderLike<T> | undefined>;
  readonly #stopped: Promise<undefined>;
  #stop!: () => void;
  #active?: ReaderLike<T>;
  #pending?: AbortController;
  #closed = false;
  #reads: Promise<void> = Promise.resolve();
  #iteratorReadInFlight = false;
  #consumer?: "iterator" | "callback";
  #iteratorOwner?: "stream" | "adapter";
  #closeReason?: StreamCloseReason;
  #closing?: Promise<void>;
  readonly #abortListener = () => void this.return().catch(reportCallbackError);

  readonly #owner: object;
  readonly #options: StreamOptions;

  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<T>>,
    owner: object,
    options: StreamOptions = {},
  ) {
    this.#owner = owner;
    this.#options = options;
    this.#stopped = new Promise((resolve) => {
      this.#stop = () => resolve(undefined);
    });
    const creation = new AbortController();
    this.#pending = creation;
    this.#reader = Promise.resolve()
      // A stream ended before its opener starts never opens: an opener
      // started with an already-aborted signal may never settle, and end()
      // waits for the opener.
      .then(() => (this.#closed ? undefined : open(creation.signal)))
      .then(async (reader) => {
        if (!reader) return undefined;
        if (this.#closed) {
          await reader.end();
          return undefined;
        }
        this.#active = reader;
        void this.#watchConnection(reader);
        return reader;
      });
    void this.#reader.catch((error: unknown) => {
      if (!this.#closed) void this.#fail(error).catch(reportCallbackError);
    });
    this.#options.signal?.addEventListener("abort", this.#abortListener, {
      once: true,
    });
    if (this.#options.signal?.aborted) this.#abortListener();
  }

  [Symbol.asyncIterator](): AsyncIterableIterator<T> {
    if (!this.#closed) {
      if (this.#consumer === "callback")
        throw new Error("reader callback consumer is active");
      if (this.#consumer === "iterator")
        throw new Error("reader iterator consumer is active");
      this.#consumer = "iterator";
      this.#iteratorOwner = "adapter";
    }
    return {
      next: () => this.#nextIterator(),
      return: () => this.return(),
      [Symbol.asyncIterator]() {
        return this;
      },
    };
  }

  #isClosed(): boolean {
    return this.#closed;
  }

  #closedResult(): IteratorResult<T> {
    if (this.#closeReason?.kind === "failed") throw this.#closeReason.error;
    return done;
  }

  async ready(): Promise<void> {
    await Promise.race([this.#reader, this.#stopped]);
    if (this.#closeReason?.kind === "failed") throw this.#closeReason.error;
  }

  #notifyClose(reason: StreamCloseReason): void {
    try {
      this.#options.onClose?.(reason);
    } catch (error) {
      reportCallbackError(error);
    }
  }

  #stopReading(): void {
    this.#closed = true;
    this.#options.signal?.removeEventListener("abort", this.#abortListener);
    this.#pending?.abort();
    this.#stop();
  }

  /** Every end, return, and failure waits for the first close's teardown. */
  #close(reason: StreamCloseReason): Promise<void> {
    this.#closing ??= this.#teardown(reason);
    return this.#closing;
  }

  async #teardown(reason: StreamCloseReason): Promise<void> {
    this.#stopReading();
    this.#closeReason = reason;
    try {
      // A late opener ends its reader before this promise settles.
      await this.#reader;
      await this.#active?.end();
    } catch {
      // A failed open, reader end, or client shutdown does not prevent
      // close. A read error remains the stream's close reason.
    }
    this.#notifyClose(reason);
  }

  async #fail(error: unknown): Promise<void> {
    await this.#close({ kind: "failed", error });
  }

  async #watchConnection(reader: ReaderLike<T>): Promise<void> {
    const callback = this.#options.onConnectionStateChange;
    if (callback === undefined || reader.connectionState === undefined) return;
    let previous: ConnectionState | undefined;
    const emit = (current: ConnectionState): void => {
      if (this.#isClosed() || previous === current) return;
      callback(previous, current);
      previous = current;
    };
    try {
      // The first state is the one read at subscription.
      let current = await reader.connectionState();
      emit(current);
      while (
        !this.#isClosed() &&
        current !== ConnectionState.Closed &&
        reader.connectionStateChanged !== undefined
      ) {
        current = await reader.connectionStateChanged(current);
        emit(current);
      }
    } catch {
      // The read path reports terminal errors to the caller and onClose.
    }
  }

  /**
   * Reads run one at a time. For messages, the next read acknowledges the prior
   * message. Await app processing before requesting the next message. Sharing
   * this iterator with an app queue does not extend the acknowledgement boundary.
   */
  next(): Promise<IteratorResult<T>> {
    if (!this.#closed) {
      if (this.#consumer === "callback")
        return Promise.reject(new Error("reader callback consumer is active"));
      if (this.#iteratorOwner === "adapter")
        return Promise.reject(new Error("reader iterator consumer is active"));
      this.#consumer = "iterator";
      this.#iteratorOwner = "stream";
    }
    return this.#nextIterator();
  }

  #nextIterator(): Promise<IteratorResult<T>> {
    if (this.#closed) return this.#next();
    if (this.#iteratorReadInFlight)
      return Promise.reject(new Error("reader iterator read is active"));
    this.#iteratorReadInFlight = true;
    return this.#next().finally(() => {
      this.#iteratorReadInFlight = false;
    });
  }

  #next(): Promise<IteratorResult<T>> {
    // A closed stream answers at once. It does not wait for a read that is
    // still ending its reader.
    const result = Promise.race([this.#reads, this.#stopped]).then(() =>
      this.#read(),
    );
    this.#reads = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  async #read(): Promise<IteratorResult<T>> {
    if (this.#closed) return this.#closedResult();
    try {
      const reader = await Promise.race([this.#reader, this.#stopped]);
      if (this.#isClosed() || reader === undefined) return this.#closedResult();
      const read = new AbortController();
      this.#pending = read;
      try {
        // Keep the host client alive while this reader is open.
        void this.#owner;
        const value = await Promise.race([
          reader.next({ signal: read.signal }),
          this.#stopped,
        ]);
        if (this.#isClosed()) return this.#closedResult();
        if (value === undefined) {
          await this.end();
          return done;
        }
        return { done: false, value };
      } finally {
        if (this.#pending === read) this.#pending = undefined;
      }
    } catch (error) {
      if (!this.#isClosed()) await this.#fail(error);
      return this.#closedResult();
    }
  }

  /**
   * Await each callback before the next read. For messages, that read acknowledges
   * the prior message. Await all processing in the callback. Work sent to an app
   * queue or an unawaited task can continue after acknowledgement.
   */
  async onValue(callback: (value: T) => void | Promise<void>): Promise<void> {
    if (this.#consumer === "iterator")
      throw new Error("reader iterator consumer is active");
    if (this.#consumer === "callback")
      throw new Error("reader callback consumer is active");
    this.#consumer = "callback";
    try {
      for (;;) {
        const item = await this.#next();
        if (item.done) return;
        await callback(item.value);
      }
    } catch (error) {
      await this.#fail(error);
      throw error;
    }
  }

  async return(): Promise<IteratorResult<T>> {
    await this.end();
    return done;
  }

  async end(): Promise<void> {
    await this.#close({ kind: "closed" });
  }
}
