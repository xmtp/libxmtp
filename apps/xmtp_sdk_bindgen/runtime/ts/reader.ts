import type { MessageReaderLike } from "../xmtp_sdk";
import type { Client } from "./client";
import type { Message } from "./message";

const done = { done: true, value: undefined } as const;

export class MessageStream implements AsyncIterableIterator<Message> {
  private readonly reader: Promise<MessageReaderLike | undefined>;
  private pending?: AbortController;
  private closed = false;

  constructor(
    open: (signal: AbortSignal) => Promise<MessageReaderLike>,
    private readonly owner: Client,
  ) {
    const creation = new AbortController();
    this.pending = creation;
    this.reader = open(creation.signal).then(async (reader) => {
      if (this.closed) {
        await reader.end();
        return undefined;
      }
      return reader;
    });
    // A caller can return before it requests the first item.
    void this.reader.catch(() => undefined);
  }

  [Symbol.asyncIterator](): AsyncIterableIterator<Message> {
    return this;
  }

  private isClosed(): boolean {
    return this.closed;
  }

  async next(): Promise<IteratorResult<Message>> {
    if (this.isClosed()) return done;
    try {
      const reader = await this.reader;
      if (this.isClosed() || reader === undefined) return done;
      const read = new AbortController();
      this.pending = read;
      try {
        const value = await reader.next({ signal: read.signal });
        return this.isClosed() || value === undefined
          ? done
          : { done: false, value };
      } finally {
        if (this.pending === read) this.pending = undefined;
      }
    } catch (error) {
      if (this.isClosed()) return done;
      this.closed = true;
      this.pending?.abort();
      try {
        const reader = await this.reader;
        await reader?.end();
      } catch {
        // Keep the read failure when opening or ending the reader also fails.
      }
      throw error;
    }
  }

  return(): Promise<IteratorResult<Message>> {
    if (this.isClosed()) return Promise.resolve(done);
    this.closed = true;
    this.pending?.abort();
    void this.reader.then((reader) => reader?.end()).catch(() => undefined);
    return Promise.resolve(done);
  }
}
