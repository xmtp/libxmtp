import type { ClientEvent, EventReaderLike } from "../../xmtp_sdk";

const done = { done: true, value: undefined } as const;

/** Each next call takes one event from the Rust queue. */
export class EventStream implements AsyncIterableIterator<ClientEvent> {
  private pending?: AbortController;
  private closed = false;

  constructor(private readonly reader: EventReaderLike) {}

  [Symbol.asyncIterator](): AsyncIterableIterator<ClientEvent> {
    return this;
  }

  private isClosed(): boolean {
    return this.closed;
  }

  async next(): Promise<IteratorResult<ClientEvent>> {
    if (this.isClosed()) return done;
    const read = new AbortController();
    this.pending = read;
    try {
      const value = await this.reader.next({ signal: read.signal });
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

  async return(): Promise<IteratorResult<ClientEvent>> {
    if (!this.closed) {
      this.closed = true;
      this.pending?.abort();
      await this.reader.end();
    }
    return done;
  }
}
