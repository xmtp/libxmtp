import {
  currentProjection,
  liftClientEvent,
  publicError,
  type ClientEvent,
} from "../../public-values.gen";
import type { EventStream as HostEventStream } from "../events/reader";

const done = { done: true, value: undefined } as const;
const hosts = new WeakMap<EventStream, HostEventStream>();
let create!: (host: HostEventStream) => EventStream;

function hostOf(stream: EventStream): HostEventStream {
  const host = hosts.get(stream);
  if (host === undefined) throw new TypeError("not an XMTP EventStream");
  return host;
}

/** Client events as public values. Each next call takes one event. */
export class EventStream implements AsyncIterableIterator<ClientEvent> {
  static {
    create = (host) => {
      const stream = new EventStream();
      hosts.set(stream, host);
      return stream;
    };
  }

  private constructor() {}

  [Symbol.asyncIterator](): AsyncIterableIterator<ClientEvent> {
    return this;
  }

  async next(): Promise<IteratorResult<ClientEvent>> {
    try {
      const item = await hostOf(this).next();
      return item.done
        ? done
        : {
            done: false,
            value: liftClientEvent(item.value, currentProjection()),
          };
    } catch (error) {
      throw publicError(error);
    }
  }

  async return(): Promise<IteratorResult<ClientEvent>> {
    try {
      await hostOf(this).return();
    } catch (error) {
      throw publicError(error);
    }
    return done;
  }
}

export function publicEventStream(host: HostEventStream): EventStream {
  return create(host);
}
