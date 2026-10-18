import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  ClientEvent,
  ConnectionState,
  EventKind,
} from "@xmtp/react-native-sdk";

import {
  DISCONNECTED_RETRYING,
  SESSION_EVENT_KINDS,
  type SessionEvent,
  type StreamReader,
  type StreamSource,
  StreamHub,
  connectionBanner,
  sessionEvent,
} from "./streamHub.ts";

/** A reader fed by the test. `next` resolves when a value or the end arrives. */
class FakeReader<T> implements StreamReader<T> {
  readonly #queue: T[] = [];
  #waiter: ((result: IteratorResult<T>) => void) | undefined;
  #reject: ((error: unknown) => void) | undefined;
  ended = false;
  nextCalls = 0;
  readyError: Error | undefined;

  push(value: T): void {
    if (this.#waiter !== undefined) {
      const waiter = this.#waiter;
      this.#waiter = undefined;
      waiter({ done: false, value });
    } else {
      this.#queue.push(value);
    }
  }

  fail(error: Error): void {
    const reject = this.#reject;
    this.#waiter = undefined;
    this.#reject = undefined;
    reject?.(error);
  }

  ready(): Promise<void> {
    return this.readyError === undefined
      ? Promise.resolve()
      : Promise.reject(this.readyError);
  }

  next(): Promise<IteratorResult<T>> {
    this.nextCalls += 1;
    if (this.ended) return Promise.resolve({ done: true, value: undefined });
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
    return Promise.resolve();
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

async function settle(condition: () => boolean): Promise<void> {
  const deadline = Date.now() + 2000;
  while (!condition() && Date.now() < deadline) await tick();
  assert.ok(condition(), "condition not reached");
}

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
  await settle(() => source.messageReaders.length === 1);
  const reader = source.messageReader;
  await settle(() => reader.nextCalls === 1);
  reader.push("m1");
  reader.push("m2");
  await settle(() => release !== undefined);
  for (let turn = 0; turn < 5; turn += 1) await tick();
  // The first listener still runs: no second read, so m1 is not acknowledged.
  assert.equal(reader.nextCalls, 1);
  assert.deepEqual(seen, ["a:m1"]);
  release?.();
  await settle(() => seen.length === 4);
  assert.deepEqual(seen, ["a:m1", "b:m1", "a:m2", "b:m2"]);
  await streams.close();
});

test("the default message reader runs only in the foreground (PR #4586)", async () => {
  const { source, streams } = hub({ foreground: false });
  streams.start();
  await settle(
    () =>
      source.conversationReaders.length === 1 &&
      source.eventReaders.length === 1,
  );
  assert.equal(source.messageReaders.length, 0);
  assert.ok(!streams.readingMessages);
  await streams.setForeground(true);
  await settle(() => source.messageReaders.length === 1);
  const first = source.messageReader;
  await streams.setForeground(false);
  // Closing the reader releases the consumer lease.
  assert.ok(first.ended);
  assert.ok(!streams.readingMessages);
  // The other readers keep running in the background.
  assert.ok(!(source.conversationReaders.at(0)?.ended ?? true));
  assert.ok(!(source.eventReaders.at(0)?.ended ?? true));
  await streams.setForeground(true);
  await settle(() => source.messageReaders.length === 2);
  assert.ok(streams.readingMessages);
  await streams.close();
  assert.ok(source.messageReaders.every((reader) => reader.ended));
});

test("rapid foreground changes apply the latest value", async () => {
  const { source, streams } = hub();
  streams.start();
  await settle(() => source.messageReaders.length === 1);
  void streams.setForeground(false);
  void streams.setForeground(true);
  await streams.setForeground(false);
  assert.ok(!streams.readingMessages);
  assert.ok(source.messageReaders.every((reader) => reader.ended));
  await streams.close();
});

test("a failed reader shows the retry banner and restarts after one second", async () => {
  await quiet(async () => {
    const { source, streams, sleeps } = hub();
    streams.start();
    await settle(() => source.messageReaders.length === 1);
    source.emitState("connected");
    assert.equal(streams.connection.getState().banner, undefined);
    const banners: (string | undefined)[] = [];
    streams.connection.subscribe(() => {
      banners.push(streams.connection.getState().banner);
    });
    await settle(() => source.messageReader.nextCalls === 1);
    source.messageReader.fail(new Error("ConsumerOwned"));
    await settle(() => source.messageReaders.length === 2);
    assert.deepEqual(sleeps, [1000]);
    assert.ok(source.messageReaders.at(0)?.ended);
    assert.equal(banners.at(0), DISCONNECTED_RETRYING);
    // The new reader reports a state, but it has not delivered anything yet.
    source.emitState("connected");
    assert.equal(streams.connection.getState().banner, DISCONNECTED_RETRYING);
    source.messageReader.push("m1");
    await settle(() => streams.connection.getState().banner === undefined);
    assert.equal(streams.connection.getState().state, "connected");
    source.emitState("reconnecting");
    assert.equal(
      streams.connection.getState().banner,
      "Reconnecting to your server…",
    );
    await streams.close();
  });
});

test("a reader that cannot open retries too", async () => {
  await quiet(async () => {
    const { source, streams, sleeps } = hub({ foreground: false });
    source.failNextMessageOpen = new Error("offline");
    streams.start();
    await streams.setForeground(true);
    await settle(() => source.messageReaders.length === 2);
    assert.deepEqual(sleeps, [1000]);
    await streams.close();
  });
});

test("an update reader failure keeps the banner until that reader reopens", async () => {
  await quiet(async () => {
    const { source, streams } = hub({ healthyAfterMs: 5 });
    streams.start();
    await settle(
      () =>
        source.conversationReaders.length === 1 &&
        source.messageReaders.length === 1,
    );
    source.emitState("connected");
    const reader = source.conversationReaders.at(0);
    await settle(() => (reader?.nextCalls ?? 0) === 1);
    const banners: (string | undefined)[] = [];
    streams.connection.subscribe(() => {
      banners.push(streams.connection.getState().banner);
    });
    reader?.fail(new Error("dropped"));
    await settle(() => source.conversationReaders.length === 2);
    await settle(() => banners.length === 2);
    assert.deepEqual(banners, [DISCONNECTED_RETRYING, undefined]);
    await streams.close();
  });
});

test("a paused or replaced reader reports no more connection states", async () => {
  const { source, streams } = hub();
  streams.start();
  await settle(() => source.messageReaders.length === 1);
  source.emitState("connected", 0);
  await streams.setForeground(false);
  source.emitState("closed", 0);
  assert.equal(streams.connection.getState().banner, undefined);
  await streams.setForeground(true);
  await settle(() => source.messageReaders.length === 2);
  source.emitState("reconnecting", 0);
  assert.equal(streams.connection.getState().banner, undefined);
  source.emitState("reconnecting", 1);
  assert.equal(
    streams.connection.getState().banner,
    "Reconnecting to your server…",
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
  await settle(() => source.eventReaders.length === 1);
  assert.deepEqual(source.kinds, SESSION_EVENT_KINDS);
  const reader = source.eventReaders.at(0);
  reader?.push({
    kind: "handle.changed",
    handle_changed: { inbox_id: "i", handle: "nick" },
  });
  reader?.push({ kind: "hmac_keys.updated", hmac_keys_updated: {} });
  reader?.push({ kind: "lagged", lagged: { discarded: 3n } });
  source.conversationReaders.at(0)?.push("c1");
  await settle(() => events.length === 2 && conversations.length === 1);
  assert.deepEqual(events, [
    { kind: "handleChanged", inboxId: "i", handle: "nick" },
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
    await settle(() => source.messageReaders.length === 1);
    source.messageReader.push("m1");
    source.messageReader.push("m2");
    await settle(() => seen.length === 2);
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
  let finished = false;
  let release: (() => void) | undefined;
  streams.onMessage(async () => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    finished = true;
  });
  streams.start();
  await settle(() => source.messageReaders.length === 1);
  source.messageReader.push("m1");
  await settle(() => release !== undefined);
  stop();
  const closing = streams.close();
  await tick();
  assert.ok(!finished);
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
    await settle(() => source.messageReaders.length === 1);
    current = false;
    source.messageReader.fail(new Error("client closed"));
    for (let turn = 0; turn < 5; turn += 1) await tick();
    assert.equal(source.messageReaders.length, 1);
    assert.deepEqual(sleeps, []);
    assert.equal(streams.connection.getState().banner, undefined);
    await streams.close();
  });
});

test("SDK events map to session events", () => {
  assert.deepEqual(
    sessionEvent({
      kind: "message.deleted",
      message_deleted: {
        group_id: new Uint8Array([0, 171]),
        message_id: new Uint8Array([15, 255]),
        cause: "deleted_locally",
      },
    }),
    {
      kind: "messageDeleted",
      conversationId: "00ab",
      messageId: "0fff",
      locallyDeleted: true,
    },
  );
  assert.deepEqual(
    sessionEvent({
      kind: "consent.changed",
      consent_changed: {
        entity_kind: "conversation",
        entity: "x",
        state: "allowed",
      },
    }),
    { kind: "changed", cause: "consent" },
  );
  assert.equal(
    sessionEvent({
      kind: "identity.registered",
      identity_registered: {
        inbox_id: "i",
        installation_key: new Uint8Array(),
      },
    }),
    undefined,
  );
  assert.deepEqual(SESSION_EVENT_KINDS, [
    "handle.changed",
    "consent.changed",
    "message.deleted",
  ]);
});

test("banner copy for each connection state", () => {
  assert.equal(connectionBanner("connected"), undefined);
  assert.equal(connectionBanner(undefined), undefined);
  assert.equal(connectionBanner("connecting"), "Connecting to your server…");
  assert.equal(
    connectionBanner("reconnecting"),
    "Reconnecting to your server…",
  );
  assert.equal(connectionBanner("failed"), "Cannot connect to your server.");
  assert.equal(connectionBanner("closed"), "Not connected to your server.");
  assert.equal(DISCONNECTED_RETRYING, "Disconnected. Retrying…");
});

test("a reader that reports connected and then fails at once keeps the retry banner (ConsumerOwned)", async () => {
  await quiet(async () => {
    const { source, streams, sleeps } = hub();
    streams.start();
    await settle(() => source.messageReaders.length === 1);
    const banners = new Set<string | undefined>();
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      // Track the banner from the first failure on.
      if (attempt === 2) {
        streams.connection.subscribe(() => {
          banners.add(streams.connection.getState().banner);
        });
      }
      source.emitState("connected");
      await settle(() => source.messageReader.nextCalls === 1);
      source.messageReader.fail(new Error("ConsumerOwned"));
      await settle(() => source.messageReaders.length === attempt + 1);
    }
    source.emitState("connected");
    assert.deepEqual(sleeps, [1000, 1000, 1000]);
    assert.equal(streams.connection.getState().banner, DISCONNECTED_RETRYING);
    assert.ok(!banners.has(undefined));
    await streams.close();
  });
});
