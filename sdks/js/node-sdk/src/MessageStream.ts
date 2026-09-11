import type {
  ConsentState,
  ConversationType,
  DeliveryCursor,
  MessageCatchUp,
} from "@xmtp/node-bindings";
import type { StreamOptions } from "@/utils/streams";

export type MessageAcknowledgement = {
  checkOwner(): boolean | Promise<boolean>;
  acknowledge(): void | Promise<void>;
  reject(): void | Promise<void>;
};

export type MessageDelivery<T> = {
  message: T;
  cursor: DeliveryCursor;
  acknowledgement: MessageAcknowledgement;
};

export type MessageReaderSource<T> = {
  nextDelivery(): Promise<MessageDelivery<T> | null | undefined>;
  close(): void | Promise<void>;
  updateScope(groupIds?: string[]): void | Promise<void>;
  updateFilter(
    conversationType?: ConversationType,
    consentStates?: ConsentState[],
  ): void | Promise<void>;
  catchUpSnapshot(): MessageCatchUp | Promise<MessageCatchUp>;
  catchUpChanged(): Promise<MessageCatchUp>;
};

/** One pending item. Construction selects callback mode or next-request acknowledgement. */
export class MessageStream<T, V> implements AsyncIterable<V> {
  #reader: MessageReaderSource<T>;
  #convert: (
    message: T,
    cursor: DeliveryCursor,
  ) => V | undefined | Promise<V | undefined>;
  #options: StreamOptions<T, V>;
  readonly #onValue?: (value: V) => void | Promise<void>;
  #pending?: MessageAcknowledgement;
  #reading = false;
  #done = false;
  #closing?: Promise<IteratorReturnResult<undefined>>;
  #cursor?: DeliveryCursor;

  constructor(
    reader: MessageReaderSource<T>,
    convert: (
      message: T,
      cursor: DeliveryCursor,
    ) => V | undefined | Promise<V | undefined>,
    options: StreamOptions<T, V> = {},
  ) {
    this.#reader = reader;
    this.#convert = convert;
    this.#options = options;
    this.#onValue = options.onValue;
    if (this.#onValue) {
      // Callback mode has no caller waiting on next(). Errors reach onError before cleanup.
      void this.#read().catch(() => undefined);
    }
  }

  get isDone() {
    return this.#done;
  }
  get deliveredCursor() {
    return this.#cursor;
  }

  #hasEnded() {
    return this.#done;
  }

  next = (): Promise<IteratorResult<V, undefined>> => {
    if (this.#onValue) {
      return Promise.reject(
        new Error("Cannot call next() on a message stream in callback mode"),
      );
    }
    return this.#read();
  };

  async #acknowledgePending() {
    const pending = this.#pending;
    if (pending) {
      // Keep the token until success so close can reject a failed acknowledgement.
      await pending.acknowledge();
      if (this.#pending === pending) this.#pending = undefined;
    }
  }

  async #read(): Promise<IteratorResult<V, undefined>> {
    if (this.#hasEnded()) return { done: true, value: undefined };
    if (this.#reading) throw new Error("A message read is already pending");
    this.#reading = true;
    try {
      await this.#acknowledgePending();
      while (!this.#hasEnded()) {
        const item = await this.#reader.nextDelivery();
        if (item === undefined || item === null) {
          await this.return();
          break;
        }
        if (this.#hasEnded()) {
          await item.acknowledgement.reject();
          break;
        }
        this.#pending = item.acknowledgement;
        // A message this client cannot decode is skipped, not fatal. Rejecting
        // it would leave the delivery cursor behind it, so every later stream
        // would select the same row and stop again, and every message stored
        // after it would be unreachable through streams.
        let value: V | undefined;
        try {
          value = await this.#convert(item.message, item.cursor);
        } catch (error) {
          this.#options.onError?.(error as Error);
          if (this.#hasEnded()) break;
          await this.#acknowledgePending();
          continue;
        }
        if (this.#hasEnded()) break;
        const checked = item.acknowledgement.checkOwner();
        const valid = typeof checked === "boolean" ? checked : await checked;
        if (this.#hasEnded()) break;
        if (!valid) {
          this.#pending = undefined;
          await item.acknowledgement.reject();
          continue;
        }
        if (value === undefined) {
          // The converter could not produce a value. That includes a failed
          // lookup, so it is not safe to acknowledge: skipping here would
          // advance the durable cursor past a message nothing has read.
          // Report it and stop, leaving the item replayable.
          throw new Error("The retained message could not be decoded");
        }
        // Do not await between the final ownership check and the app handoff.
        this.#cursor = item.cursor;
        if (this.#onValue) {
          await this.#onValue(value);
          if (this.#hasEnded()) break;
          await this.#acknowledgePending();
        } else {
          return { done: false, value };
        }
      }
      return { done: true, value: undefined };
    } catch (error) {
      if (!this.#hasEnded()) {
        try {
          this.#options.onError?.(error as Error);
        } finally {
          await this.return();
        }
      }
      throw error;
    } finally {
      this.#reading = false;
    }
  }

  async #close(
    pending?: MessageAcknowledgement,
  ): Promise<IteratorReturnResult<undefined>> {
    const invoke = (action: () => void | Promise<void>): Promise<void> => {
      try {
        return Promise.resolve(action());
      } catch (error) {
        return Promise.reject(
          error instanceof Error
            ? error
            : new Error("Message stream close failed", { cause: error }),
        );
      }
    };
    // Invoke close before any await so it fences pending reads and host handoffs.
    const results = await Promise.allSettled([
      invoke(() => this.#reader.close()),
      invoke(() => pending?.reject()),
    ]);
    try {
      for (const result of results) {
        if (result.status === "rejected") throw result.reason;
      }
      return { done: true, value: undefined };
    } finally {
      this.#options.onEnd?.();
    }
  }

  return = (): Promise<IteratorReturnResult<undefined>> => {
    if (this.#done) {
      return this.#closing ?? Promise.resolve({ done: true, value: undefined });
    }
    this.#done = true;
    const pending = this.#pending;
    this.#pending = undefined;
    this.#closing = this.#close(pending);
    return this.#closing;
  };

  end = this.return;
  updateScope = (groupIds?: string[]) => this.#reader.updateScope(groupIds);
  updateFilter = (
    conversationType?: ConversationType,
    consentStates?: ConsentState[],
  ) => this.#reader.updateFilter(conversationType, consentStates);
  catchUpSnapshot = () => this.#reader.catchUpSnapshot();
  catchUpChanged = () => this.#reader.catchUpChanged();
  [Symbol.asyncIterator]() {
    return this;
  }
}
