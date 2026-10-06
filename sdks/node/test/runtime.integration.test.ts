// Node host behavior that Rust cannot see: the generated public projection,
// weak client ownership, app codecs in a live stream and iterator exit. Rust
// covers the reader, codec and client logic itself.
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  createRegisteredClient,
  createSigner,
  endAfterTest,
} from "@test/helpers";
import {
  Archives,
  Backend,
  Client,
  Conversations,
  Diagnostics,
  Dm,
  Group,
  MarkdownCodec,
  Message,
  Preferences,
  ReadReceiptCodec,
  SignatureRequest,
  Storage,
  Timestamp,
  XmtpError,
  encodeText,
  type ConnectionState,
  type ContentCodec,
  type StreamCloseReason,
} from "@xmtp/node-sdk";
import { expect, it, vi } from "vitest";

// The public options adapter, driven with a fake reader of binding states.
import { hostOptions } from "../dist/runtime/public/streams.js";
import { ReaderStream } from "../dist/runtime/streams/reader.js";
import { ConnectionState as BoundState } from "../dist/xmtp_sdk.js";

const PUBLIC_OBJECTS = [
  Client,
  Conversations,
  Group,
  Dm,
  Preferences,
  Diagnostics,
  Archives,
  Storage,
  Backend,
  SignatureRequest,
];
const PLAIN = new Set<unknown>([
  Object.prototype,
  null,
  Array.prototype,
  Map.prototype,
  Set.prototype,
  Message.prototype,
]);
// Public fields whose type is a string union. A number is a binding enum.
const UNIONS = new Set([
  "kind",
  "deliveryStatus",
  "permissionLevel",
  "consentState",
  "membershipState",
  "conversationType",
  "state",
  "category",
]);

/** A public value is plain data: no binding class, tag, payload or enum. */
function expectPlain(value: unknown, path: string, seen = new Set()): void {
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  expect(value, path).not.toBeInstanceOf(ArrayBuffer);
  if (value instanceof Uint8Array || value instanceof Timestamp) return;
  if (PUBLIC_OBJECTS.some((type) => value instanceof type)) return;
  expect(PLAIN.has(Object.getPrototypeOf(value)), `${path} prototype`).toBe(
    true,
  );
  expect("tag" in value || "inner" in value, `${path} binding shape`).toBe(
    false,
  );
  const entries = value instanceof Map ? [...value] : Object.entries(value);
  for (const [key, item] of entries) {
    if (UNIONS.has(String(key)))
      expect(typeof item, `${path}.${key}`).not.toBe("number");
    expectPlain(item, `${path}.${String(key)}`, seen);
  }
}

it("lifts every returned value to plain public data", async () => {
  // A 64-bit option keeps its exact value.
  const interval = 2n ** 53n + 1n;
  const alix = endAfterTest(
    await createRegisteredClient(createSigner().signer, {
      workers: { defaultIntervalNs: interval },
    }),
  );
  expect(alix.options.workers?.defaultIntervalNs).toBe(interval);
  const bo = endAfterTest(await createRegisteredClient(createSigner().signer));
  const group = await alix.conversations.createGroup([bo.identity]);
  // The generated member guard rejects a list that mixes inbox IDs and
  // identities before it calls Rust.
  await expect(
    group.addMembers([bo.inboxId, bo.identity] as never),
  ).rejects.toBeInstanceOf(XmtpError.InvalidArgument);
  const id = await group.sendText("plain");
  const message = (await group.messages()).find((item) => item.id === id)!;
  // The hand-written Message actions route by the message's own IDs.
  const reply = (await alix.conversations.getMessageById(
    await message.reply("reply"),
  ))!;
  const conversation = await reply.conversation();
  expect(conversation).toBeInstanceOf(Group);
  expect(conversation?.id).toBe(group.id);
  expect((await reply.parent())?.id).toBe(id);
  const reaction = await message.react({
    action: "added",
    schema: "unicode",
    content: "👍",
  });
  expect((await message.refresh())?.reactions.map((item) => item.id)).toEqual([
    reaction,
  ]);
  for (const [path, value] of Object.entries({
    messages: await group.messages(),
    members: await group.members(),
    state: await group.state(),
    options: alix.options,
    inboxState: await alix.inboxState(false),
    configuration: alix.serverConfiguration,
  }))
    expectPlain(value, path);
  await bo.end();
  await alix.end();
  expect(() => message.client()).toThrow(XmtpError.ClientClosed);
});

it("a dropped client is collected and its messages report ClientClosed", async () => {
  const fixture = fileURLToPath(
    new URL("./fixtures/client-collection.mjs", import.meta.url),
  );
  const { stdout } = await promisify(execFile)(
    process.execPath,
    ["--expose-gc", fixture],
    { env: process.env, timeout: 60_000 },
  );
  expect(stdout).toContain("PASS client collection");
});

it("a standard codec rejects another kind's value with InvalidArgument", () => {
  expect(() => new MarkdownCodec().decode(encodeText("text"))).toThrow(
    XmtpError.InvalidArgument,
  );
  expect(() => new ReadReceiptCodec().encode("wrong" as never)).toThrow(
    XmtpError.InvalidArgument,
  );
});

// verifies: PROC-045, CTYPE-009, PROC-044
it("a stream keeps hostile codec failures typed and delivers the next item", async () => {
  const type = {
    authorityId: "example.org",
    typeId: "hostile",
    versionMajor: 1,
    versionMinor: 0,
  };
  const codec: ContentCodec<string> = {
    type,
    encode: (value) => ({
      type,
      parameters: new Map(),
      content: new TextEncoder().encode(value),
    }),
    decode(encoded) {
      const value = new TextDecoder().decode(encoded.content);
      if (value === "null prototype") throw Object.create(null);
      if (value === "throwing toString")
        // The app may throw anything; the SDK must not call its toString.
        // oxlint-disable-next-line typescript/only-throw-error
        throw {
          toString: () => {
            throw new Error("diagnostic failed");
          },
        };
      return value;
    },
  };
  const client = endAfterTest(
    await createRegisteredClient(createSigner().signer, {
      codecs: [codec],
    }),
  );
  const group = await client.conversations.createGroup([]);
  const states: [ConnectionState | undefined, ConnectionState][] = [];
  const stream = group.streamMessages({
    onConnectionStateChange: (previous, current) =>
      states.push([previous, current]),
  });
  await stream.ready();
  for (const hostile of ["null prototype", "throwing toString"]) {
    const failed = await group.send(codec.encode(hostile));
    const good = await group.send(codec.encode(`after ${hostile}`));
    const item = (await stream.next()).value!;
    expect(item.id).toBe(failed);
    expect(Reflect.get(item.content, "value")).toBeUndefined();
    expect(item.content).toMatchObject({
      kind: "custom",
      error: {
        code: "CodecDecodeFailed",
        category: "callback",
        retryable: false,
        message: "custom content codec failed",
      },
    });
    expect((await stream.next()).value).toMatchObject({
      id: good,
      content: { kind: "custom", value: `after ${hostile}` },
    });
  }
  await stream.end();
  // The public stream reports the state at subscription with no previous state.
  expect(states[0]?.[0]).toBeUndefined();
  expect(["connecting", "connected"]).toContain(states[0]?.[1]);
  await client.end();
});

// verifies: PROC-044
it("public stream options lift the previous and current connection states", async () => {
  const states: [ConnectionState | undefined, ConnectionState][] = [];
  const changes: ((state: BoundState) => void)[] = [];
  const stream = endAfterTest(
    new ReaderStream(
      async () => ({
        next: () => new Promise<undefined>(() => {}),
        end: async () => {},
        connectionState: async () => BoundState.Connected,
        connectionStateChanged: () =>
          new Promise<BoundState>((resolve) => changes.push(resolve)),
      }),
      {},
      hostOptions({
        onConnectionStateChange: (previous, current) =>
          states.push([previous, current]),
      }),
    ),
  );
  await stream.ready();
  for (const next of [BoundState.Reconnecting, BoundState.Connected]) {
    await vi.waitFor(() => expect(changes).toHaveLength(1));
    changes.shift()!(next);
  }
  await vi.waitFor(() => expect(states).toHaveLength(3));
  expect(states).toEqual([
    [undefined, "connected"],
    ["connected", "reconnecting"],
    ["reconnecting", "connected"],
  ]);
  await stream.end();
});

it("a public stream read after client end fails with the public ClientClosed", async () => {
  const client = endAfterTest(
    await createRegisteredClient(createSigner().signer),
  );
  const group = await client.conversations.createGroup([]);
  const reasons: StreamCloseReason[] = [];
  const stream = group.streamMessages({
    onClose: (reason) => reasons.push(reason),
  });
  await stream.ready();
  await client.end();
  const error: unknown = await stream.next().catch((failure) => failure);
  // The public reader lifts the binding error; no binding shape leaks.
  expect(error).toBeInstanceOf(XmtpError.ClientClosed);
  expect(error).toMatchObject({
    details: { code: "ClientClosed", category: "lifecycle" },
  });
  expect("tag" in Object(error) || "inner" in Object(error)).toBe(false);
  await vi.waitFor(() => expect(reasons).toHaveLength(1));
  expect(reasons).toEqual([{ kind: "failed", error }]);
});

// verifies: PROC-052, PROC-031, PROC-041
it.each(["break", "throw", "abort"] as const)(
  "an iterator %s closes the stream once and leaves the held item unacknowledged",
  async (mode) => {
    const client = endAfterTest(
      await createRegisteredClient(createSigner().signer),
    );
    const group = await client.conversations.createGroup([]);
    const id = await group.sendText("held");
    const controller = new AbortController();
    const reasons: StreamCloseReason[] = [];
    const closed = Promise.withResolvers<void>();
    const received = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    const stream = group.streamMessages({
      signal: controller.signal,
      onClose: (reason) => (reasons.push(reason), closed.resolve()),
    });
    const appError = new Error("app failed");
    const consumer = (async () => {
      for await (const message of stream) {
        expect(message.id).toBe(id);
        received.resolve();
        await release.promise;
        if (mode === "throw") throw appError;
        break;
      }
    })().catch((error: unknown) => {
      if (error !== appError) throw error;
    });
    await received.promise;
    if (mode === "abort") controller.abort();
    else release.resolve();
    await closed.promise;
    // Reopen before an aborted body returns: only automatic cleanup can
    // have released the reader.
    const replay = await group.messageReader();
    expect((await replay.next())?.id).toBe(id);
    await replay.end();
    release.resolve();
    await consumer;
    expect(reasons).toEqual([{ kind: "closed" }]);
    await client.end();
  },
);

/** Rejects when `promise` does not settle in time, so a deadlock fails. */
function within<T>(promise: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 10_000);
    }),
  ]).finally(() => clearTimeout(timer));
}

// A listener callback runs on the Node thread while Rust waits for it. It can
// stop its own listener and end its client from inside the callback.
// verifies: EVENT-052
it("a listener callback can stop its listener and end its client", async () => {
  const client = endAfterTest(
    await createRegisteredClient(createSigner().signer),
  );
  const filter = { kinds: ["conversation.joined" as const] };
  const stopped = Promise.withResolvers<void>();
  let stopCalls = 0;
  const id: bigint = await client.startListener(filter, async () => {
    stopCalls += 1;
    await client.stopListener(id);
    stopped.resolve();
  });
  await client.conversations.createGroup([]);
  await within(stopped.promise, "stop inside listener");

  const ended = Promise.withResolvers<void>();
  await client.startListener(filter, async () => {
    await client.end();
    ended.resolve();
  });
  // The end inside the callback can close this call first.
  await client.conversations.createGroup([]).catch(() => undefined);
  await within(ended.promise, "end inside listener");
  expect(stopCalls).toBe(1);
  await expect(client.conversations.createGroup([])).rejects.toBeInstanceOf(
    XmtpError.ClientClosed,
  );
});
