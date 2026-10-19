import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import type {
  ClientEvent,
  ConnectionState,
  EventKind,
} from "@xmtp/react-native-sdk";

import {
  ConnectionCopy,
  RETRY_DELAY_MS,
  type SessionEvent,
  type StreamReader,
  type StreamSource,
  StreamHub,
  connectionBanner,
} from "./streamHub.ts";

/** A reader fed by the test. `next` resolves when a value or the end arrives. */
class FakeReader<T> implements StreamReader<T> {
  readonly #queue: T[] = [];
  #waiter: ((result: IteratorResult<T>) => void) | undefined;
  #reject: ((error: unknown) => void) | undefined;
  #error: Error | undefined;
  ended = false;
  nextCalls = 0;
  readyError: Error | undefined;
  /** Holds `end()` open until it resolves (a slow SDK close). */
  endGate: Promise<void> | undefined;

  push(value: T): void {
    if (this.#waiter !== undefined) {
      const waiter = this.#waiter;
      this.#waiter = undefined;
      waiter({ done: false, value });
    } else {
      this.#queue.push(value);
    }
  }

  /** Fails the pending read, or the next one when none is pending. */
  fail(error: Error): void {
    const reject = this.#reject;
    this.#waiter = undefined;
    this.#reject = undefined;
    if (reject === undefined) this.#error = error;
    else reject(error);
  }

  ready(): Promise<void> {
    return this.readyError === undefined
      ? Promise.resolve()
      : Promise.reject(this.readyError);
  }

  next(): Promise<IteratorResult<T>> {
    this.nextCalls += 1;
    if (this.ended) return Promise.resolve({ done: true, value: undefined });
    const error = this.#error;
    this.#error = undefined;
    if (error !== undefined) return Promise.reject(error);
    const value = this.#queue.shift();
    if (value !== undefined) return Promise.resolve({ done: false, value });
    return new Promise((resolve, reject) => {
      this.#waiter = resolve;
      this.#reject = reject;
    });
  }

  end(): Promise<void> {
    this.ended = true;
    const waiter = this.#waiter;
    this.#waiter = undefined;
    waiter?.({ done: true, value: undefined });
    return this.endGate ?? Promise.resolve();
  }
}

class FakeSource implements StreamSource<string, string> {
  readonly messageReaders: FakeReader<string>[] = [];
  readonly conversationReaders: FakeReader<string>[] = [];
  readonly eventReaders: FakeReader<ClientEvent>[] = [];
  readonly stateCallbacks: ((state: ConnectionState) => void)[] = [];
  kinds: readonly EventKind[] = [];
  failNextMessageOpen: Error | undefined;

  messages(onState: (state: ConnectionState) => void): StreamReader<string> {
    const reader = new FakeReader<string>();
    if (this.failNextMessageOpen !== undefined) {
      reader.readyError = this.failNextMessageOpen;
      this.failNextMessageOpen = undefined;
    }
    this.messageReaders.push(reader);
    this.stateCallbacks.push(onState);
    return reader;
  }

  conversations(): StreamReader<string> {
    const reader = new FakeReader<string>();
    this.conversationReaders.push(reader);
    return reader;
  }

  events(kinds: readonly EventKind[]): Promise<StreamReader<ClientEvent>> {
    this.kinds = kinds;
    const reader = new FakeReader<ClientEvent>();
    this.eventReaders.push(reader);
    return Promise.resolve(reader);
  }

  get messageReader(): FakeReader<string> {
    const reader = this.messageReaders.at(-1);
    assert.ok(reader !== undefined, "no message reader");
    return reader;
  }

  emitState(
    state: ConnectionState,
    index = this.stateCallbacks.length - 1,
  ): void {
    this.stateCallbacks.at(index)?.(state);
  }
}

const tick = (): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, 0));

/** Runs zero-delay turns until `condition` holds (readers chain several). */
async function waitFor(condition: () => boolean): Promise<void> {
  for (let turn = 0; turn < 100 && !condition(); turn += 1) await tick();
  assert.ok(condition(), "condition not reached");
}

// A failed assertion must not leave a hub open: its healthy timer would hold
// the process for a minute.
const open: StreamHub<string, string>[] = [];
afterEach(
  async () => {
    await Promise.all(open.splice(0).map((streams) => streams.close()));
  },
  { timeout: 2000 },
);

function hub(
  options: {
    foreground?: boolean;
    current?: () => boolean;
    healthyAfterMs?: number;
  } = {},
) {
  const source = new FakeSource();
  const sleeps: number[] = [];
  const streams = new StreamHub(source, {
    foreground: options.foreground ?? true,
    isCurrent: options.current ?? (() => true),
    healthyAfterMs: options.healthyAfterMs ?? 60_000,
    sleep: (ms) => {
      sleeps.push(ms);
      return Promise.resolve();
    },
  });
  open.push(streams);
  return { source, streams, sleeps };
}

const quiet = async (body: () => Promise<void>): Promise<void> => {
  const warn = console.warn;
  console.warn = () => undefined;
  try {
    await body();
  } finally {
    console.warn = warn;
  }
};

test("each message reaches every listener before the next read acknowledges it", async () => {
  const { source, streams } = hub();
  const seen: string[] = [];
  let release: (() => void) | undefined;
  streams.onMessage(async (message) => {
    seen.push(`a:${message}`);
    if (message === "m1") {
      await new Promise<void>((resolve) => {
        release = resolve;
      });
    }
  });
  streams.onMessage((message) => {
    seen.push(`b:${message}`);
  });
  streams.start();
  await waitFor(() => source.messageReaders.length === 1);
  const reader = source.messageReader;
  await waitFor(() => reader.nextCalls === 1);
  reader.push("m1");
  reader.push("m2");
  await waitFor(() => release !== undefined);
  for (let turn = 0; turn < 5; turn += 1) await tick();
  // A listener still runs: no second read, so m1 is not acknowledged.
  assert.equal(reader.nextCalls, 1);
  assert.ok(!seen.some((entry) => entry.endsWith(":m2")));
  release?.();
  await waitFor(() => seen.length === 4);
  for (const listener of ["a", "b"]) {
    assert.deepEqual(
      seen.filter((entry) => entry.startsWith(`${listener}:`)),
      [`${listener}:m1`, `${listener}:m2`],
    );
  }
  await streams.close();
});

test("the default message reader runs only in the foreground, and a quick return restarts it", async () => {
  const { source, streams } = hub({ foreground: false });
  streams.start();
  await waitFor(
    () =>
      source.conversationReaders.length === 1 &&
      source.eventReaders.length === 1,
  );
  assert.equal(source.messageReaders.length, 0);
  assert.ok(!streams.readingMessages);
  await streams.setForeground(true);
  await waitFor(() => source.messageReaders.length === 1);
  const first = source.messageReader;
  await streams.setForeground(false);
  // Closing the reader releases the consumer lease.
  assert.ok(first.ended);
  assert.ok(!streams.readingMessages);
  // The other readers keep running in the background.
  assert.ok(!(source.conversationReaders.at(0)?.ended ?? true));
  assert.ok(!(source.eventReaders.at(0)?.ended ?? true));
  await streams.setForeground(true);
  await waitFor(() => source.messageReaders.length === 2);
  assert.ok(streams.readingMessages);
  // Back to the foreground while the background close is still running: the
  // reader ends up in the state of the last call.
  let closed: (() => void) | undefined;
  source.messageReader.endGate = new Promise((resolve) => {
    closed = resolve;
  });
  void streams.setForeground(false);
  await tick();
  const foreground = streams.setForeground(true);
  closed?.();
  await foreground;
  for (let turn = 0; turn < 3; turn += 1) await tick();
  assert.ok(streams.readingMessages);
  assert.ok(!source.messageReader.ended);
  await streams.close();
  assert.ok(source.messageReaders.every((reader) => reader.ended));
});

test("a failed reader shows the retry banner until its replacement delivers, and restarts after the retry delay", async () => {
  await quiet(async () => {
    const { source, streams, sleeps } = hub();
    const banners: (string | undefined)[] = [];
    streams.connection.subscribe(() => {
      banners.push(streams.connection.getState().banner);
    });
    // The first reader cannot open.
    source.failNextMessageOpen = new Error("offline");
    streams.start();
    await waitFor(() => source.messageReaders.length === 2);
    assert.equal(streams.connection.getState().banner, ConnectionCopy.retrying);
    // The next reports connected and then fails at once (ConsumerOwned).
    source.emitState("connected");
    await waitFor(() => source.messageReader.nextCalls === 1);
    source.messageReader.fail(new Error("ConsumerOwned"));
    await waitFor(() => source.messageReaders.length === 3);
    assert.deepEqual(sleeps, [RETRY_DELAY_MS, RETRY_DELAY_MS]);
    assert.ok(
      source.messageReaders.slice(0, 2).every((reader) => reader.ended),
    );
    // A state report alone does not clear the banner; a delivered value does.
    source.emitState("connected");
    assert.equal(streams.connection.getState().banner, ConnectionCopy.retrying);
    assert.ok(!banners.includes(undefined));
    source.messageReader.push("m1");
    await waitFor(() => streams.connection.getState().banner === undefined);
    assert.equal(streams.connection.getState().state, "connected");
    source.emitState("reconnecting");
    assert.equal(
      streams.connection.getState().banner,
      connectionBanner("reconnecting"),
    );
    await streams.close();
  });
});

test("an update reader failure keeps the banner until that reader reopens", async () => {
  await quiet(async () => {
    const { source, streams } = hub({ healthyAfterMs: 0 });
    streams.start();
    await waitFor(
      () =>
        source.conversationReaders.length === 1 &&
        source.messageReaders.length === 1,
    );
    source.emitState("connected");
    const reader = source.conversationReaders.at(0);
    await waitFor(() => (reader?.nextCalls ?? 0) === 1);
    const banners: (string | undefined)[] = [];
    streams.connection.subscribe(() => {
      banners.push(streams.connection.getState().banner);
    });
    reader?.fail(new Error("dropped"));
    await waitFor(() => source.conversationReaders.length === 2);
    await waitFor(() => banners.length === 2);
    assert.deepEqual(banners, [ConnectionCopy.retrying, undefined]);
    await streams.close();
  });
});

test("a paused or replaced reader reports no more connection states", async () => {
  const { source, streams } = hub();
  streams.start();
  await waitFor(() => source.messageReaders.length === 1);
  source.emitState("connected", 0);
  await streams.setForeground(false);
  source.emitState("closed", 0);
  assert.equal(streams.connection.getState().banner, undefined);
  await streams.setForeground(true);
  await waitFor(() => source.messageReaders.length === 2);
  source.emitState("reconnecting", 0);
  assert.equal(streams.connection.getState().banner, undefined);
  source.emitState("reconnecting", 1);
  assert.equal(
    streams.connection.getState().banner,
    connectionBanner("reconnecting"),
  );
  await streams.close();
});

test("events reach listeners as session events; other kinds are skipped", async () => {
  const { source, streams } = hub();
  const events: SessionEvent[] = [];
  const conversations: string[] = [];
  streams.onEvent((event) => {
    events.push(event);
  });
  streams.onConversation((conversation) => {
    conversations.push(conversation);
  });
  streams.start();
  await waitFor(() => source.eventReaders.length === 1);
  const delivered: ClientEvent[] = [
    {
      kind: "handle.changed",
      handle_changed: { inbox_id: "i", handle: "nick" },
    },
    {
      kind: "message.deleted",
      message_deleted: {
        group_id: new Uint8Array([0, 171]),
        message_id: new Uint8Array([15, 255]),
        cause: "deleted_locally",
      },
    },
    {
      kind: "consent.changed",
      consent_changed: {
        entity_kind: "conversation",
        entity: "x",
        state: "allowed",
      },
    },
  ];
  // The session subscribes to every kind it maps.
  for (const event of delivered) assert.ok(source.kinds.includes(event.kind));
  const reader = source.eventReaders.at(0);
  for (const event of delivered) reader?.push(event);
  reader?.push({ kind: "hmac_keys.updated", hmac_keys_updated: {} });
  reader?.push({ kind: "lagged", lagged: { discarded: 3n } });
  source.conversationReaders.at(0)?.push("c1");
  await waitFor(() => events.length === 4 && conversations.length === 1);
  assert.deepEqual(events, [
    { kind: "handleChanged", inboxId: "i", handle: "nick" },
    {
      kind: "messageDeleted",
      conversationId: "00ab",
      messageId: "0fff",
      locallyDeleted: true,
    },
    { kind: "changed", cause: "consent" },
    { kind: "changed", cause: "lagged" },
  ]);
  assert.deepEqual(conversations, ["c1"]);
  await streams.close();
});

test("a failing listener does not stop delivery or other listeners", async () => {
  await quiet(async () => {
    const { source, streams } = hub();
    const seen: string[] = [];
    streams.onMessage(() => {
      throw new Error("listener bug");
    });
    streams.onMessage((message) => {
      seen.push(message);
    });
    streams.start();
    await waitFor(() => source.messageReaders.length === 1);
    source.messageReader.push("m1");
    source.messageReader.push("m2");
    await waitFor(() => seen.length === 2);
    assert.equal(source.messageReaders.length, 1);
    await streams.close();
  });
});

test("unsubscribed listeners stop receiving; close waits for running listeners", async () => {
  const { source, streams } = hub();
  const seen: string[] = [];
  const stop = streams.onMessage((message) => {
    seen.push(message);
  });
  const slow: string[] = [];
  let finished = false;
  let release: (() => void) | undefined;
  streams.onMessage(async (message) => {
    slow.push(message);
    if (message !== "m2") return;
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    finished = true;
  });
  streams.start();
  await waitFor(() => source.messageReaders.length === 1);
  source.messageReader.push("m1");
  await waitFor(() => slow.length === 1);
  stop();
  source.messageReader.push("m2");
  await waitFor(() => release !== undefined);
  let closed = false;
  const closing = streams.close().then(() => {
    closed = true;
  });
  for (let turn = 0; turn < 5; turn += 1) await tick();
  assert.ok(!closed);
  release?.();
  await closing;
  assert.ok(finished);
  assert.deepEqual(seen, ["m1"]);
  // Nothing restarts after close.
  await streams.setForeground(true);
  assert.equal(source.messageReaders.length, 1);
});

test("readers stop and do not restart once the session is no longer current", async () => {
  await quiet(async () => {
    let current = true;
    const { source, streams, sleeps } = hub({ current: () => current });
    streams.start();
    await waitFor(() => source.messageReaders.length === 1);
    current = false;
    source.messageReader.fail(new Error("client closed"));
    for (let turn = 0; turn < 5; turn += 1) await tick();
    assert.equal(source.messageReaders.length, 1);
    assert.deepEqual(sleeps, []);
    assert.equal(streams.connection.getState().banner, undefined);
    await streams.close();
  });
});

test("every connection state except connected shows its own banner", () => {
  assert.equal(connectionBanner("connected"), undefined);
  assert.equal(connectionBanner(undefined), undefined);
  const shown = (
    ["connecting", "reconnecting", "failed", "closed"] as const
  ).map(connectionBanner);
  assert.ok(
    shown.every(
      (text) => text !== undefined && text !== ConnectionCopy.retrying,
    ),
  );
  assert.equal(new Set(shown).size, shown.length);
});
