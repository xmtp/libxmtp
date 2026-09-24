import type { MessageReaderLike } from "../xmtp_sdk";
import type { Client } from "./client";
import type { Message } from "./message";

const done = { done: true, value: undefined } as const;

export class MessageStream implements AsyncIterableIterator<Message> {
  private readonly reader: Promise<MessageReaderLike>;
  private pending?: AbortController;
  private closed = false;

  constructor(
    open: (signal: AbortSignal) => Promise<MessageReaderLike>,
    private readonly owner: Client,
  ) {
    const creation = new AbortController();
    this.pending = creation;
    this.reader = open(creation.signal).then(async (reader) => {
      if (this.closed) await reader.end();
      return reader;
    });
  }

  [Symbol.asyncIterator](): AsyncIterableIterator<Message> {
    return this;
  }

  private isClosed(): boolean {
    return this.closed;
  }

  async next(): Promise<IteratorResult<Message>> {
    if (this.isClosed()) return done;
    const reader = await this.reader;
    if (this.isClosed()) return done;
    const read = new AbortController();
    this.pending = read;
    try {
      const value = await reader.next({ signal: read.signal });
      return this.isClosed() || value === undefined
        ? done
        : { done: false, value };
    } catch (error) {
      if (this.isClosed()) return done;
      throw error;
    } finally {
      if (this.pending === read) this.pending = undefined;
    }
  }

  return(): Promise<IteratorResult<Message>> {
    if (this.isClosed()) return Promise.resolve(done);
    this.closed = true;
    this.pending?.abort();
    void this.reader.then((reader) => reader.end());
    return Promise.resolve(done);
  }
}
