import { ConnectionState } from "../../xmtp_sdk";
import type {
  ClientLike,
  Conversation,
  ConversationReaderOptions,
  MessageReaderOptions,
  ConversationMessageReaderOptions,
} from "../../xmtp_sdk";
import type { Client } from "../client";
import type { Message } from "../message";

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

type MessageReaderSource<T, Selection> = {
  messageReader(
    selection?: Selection,
    transport?: { signal: AbortSignal },
  ): Promise<ReaderLike<T>>;
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
  #callbackConsumer = false;
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
    return this;
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
   * Reads run one at a time. The next read acknowledges the prior value, so a
   * second read must not start while the first value has not reached the app.
   */
  next(): Promise<IteratorResult<T>> {
    if (this.#callbackConsumer && !this.#closed)
      return Promise.reject(new Error("reader callback consumer is active"));
    return this.#next();
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

  /** Resolve after the callback; the next read then acknowledges this value. */
  async onValue(callback: (value: T) => void | Promise<void>): Promise<void> {
    if (this.#callbackConsumer)
      throw new Error("reader callback consumer is active");
    this.#callbackConsumer = true;
    try {
      for (;;) {
        const item = await this.#next();
        if (item.done) return;
        await callback(item.value);
      }
    } catch (error) {
      await this.#fail(error);
      throw error;
    } finally {
      this.#callbackConsumer = false;
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

export class MessageStream<T = Message> extends ReaderStream<T> {
  static open<T>(
    owner: { conversations(): MessageReaderSource<T, MessageReaderOptions> },
    selection?: MessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream<T> {
    return new MessageStream(
      (signal) => owner.conversations().messageReader(selection, { signal }),
      owner,
      options,
    );
  }

  static openGroup<T>(
    owner: object,
    group: MessageReaderSource<T, ConversationMessageReaderOptions>,
    selection?: ConversationMessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream<T> {
    return new MessageStream(
      (signal) => group.messageReader(selection, { signal }),
      owner,
      options,
    );
  }

  static openDm<T>(
    owner: object,
    dm: MessageReaderSource<T, ConversationMessageReaderOptions>,
    selection?: ConversationMessageReaderOptions,
    options?: StreamOptions,
  ): MessageStream<T> {
    return new MessageStream(
      (signal) => dm.messageReader(selection, { signal }),
      owner,
      options,
    );
  }

  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<T>>,
    owner: object,
    options?: StreamOptions,
  ) {
    super(open, owner, options);
  }
}

export class ConversationStream extends ReaderStream<Conversation> {
  static open(
    owner: Client,
    selection?: ConversationReaderOptions,
    options?: StreamOptions,
  ): ConversationStream {
    return new ConversationStream(
      (signal) =>
        owner.conversations().conversationReader(selection, { signal }),
      owner,
      options,
    );
  }

  static openBrowser(
    owner: ClientLike,
    selection?: ConversationReaderOptions,
    options?: StreamOptions,
  ): ConversationStream {
    return new ConversationStream(
      (signal) =>
        owner.conversations().conversationReader(selection, { signal }),
      owner,
      options,
    );
  }

  constructor(
    open: (signal: AbortSignal) => Promise<ReaderLike<Conversation>>,
    owner: object,
    options?: StreamOptions,
  ) {
    super(open, owner, options);
  }
}
