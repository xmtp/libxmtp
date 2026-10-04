// The public TypeScript layer over the Node binding: public errors, streams,
// events, listeners, codecs, callbacks, and logging.
import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import {
  currentProjection,
  lowerClientOptions,
} from "../../../../target/sdk-conformance/typescript-napi/public-values.gen.ts";
// Internals for the connection-state and options checks below.
import { hostOptions } from "../../../../target/sdk-conformance/typescript-napi/runtime/public/streams.ts";
import { MessageStream as HostMessageStream } from "../../../../target/sdk-conformance/typescript-napi/runtime/streams/reader.ts";
import { ConnectionState as BoundState } from "../../../../target/sdk-conformance/typescript-napi/xmtp_sdk.ts";

const viemRoot = realpathSync(
  fileURLToPath(
    new URL("../../../../sdks/node/node_modules/viem", import.meta.url),
  ),
);
const { generatePrivateKey, privateKeyToAccount } = await import(
  pathToFileURL(join(viemRoot, "_esm/accounts/index.js")).href
);
const { toBytes } = await import(
  pathToFileURL(join(viemRoot, "_esm/index.js")).href
);

function signerFor(): sdk.Signer {
  const account = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { kind: "ethereum", identifier: account.address.toLowerCase() };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      const signature = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signature)) };
    },
  };
}

const backend: sdk.BackendOptions = { url: process.env.XMTP_BACKEND_URL! };

async function options(
  codecs: readonly sdk.AnyContentCodec[] = [],
): Promise<sdk.ClientOptions> {
  const directory = await mkdtemp(join(tmpdir(), "xmtp-public-streams-"));
  return {
    backend,
    storage: {
      location: {
        dbPath: join(directory, "client.db"),
        attachmentsDir: join(directory, "attachments"),
      },
    },
    deviceSync: false,
    codecs,
  };
}

// A public error: the subclass of its code, plain details, and no binding
// tag or payload.
function publicError(
  type: abstract new (...args: never[]) => sdk.XmtpError,
  code: string,
  category: string,
): (error: unknown) => boolean {
  return (error) => {
    assert.ok(error instanceof sdk.XmtpError, `not an XmtpError: ${error}`);
    assert.ok(error instanceof type, `not ${code}: ${error.name}`);
    assert.equal(error.details.code, code);
    assert.equal(error.details.category, category);
    assert.equal(typeof error.details.retryable, "boolean");
    assert.ok(!("tag" in error) && !("inner" in error), "binding error shape");
    return true;
  };
}

// Before initLogging, installing or clearing a log sink fails with the public
// error, not the binding class.
for (const install of [
  () => sdk.setLogSink({ log: async () => {} }),
  () => sdk.setLogSink(),
])
  await assert.rejects(
    install,
    publicError(sdk.XmtpError.InvalidInput, "InvalidInput", "input"),
  );

// A custom codec over public values.
type Point = { x: number; y: number };
const pointType: sdk.ContentTypeId = {
  authorityId: "example.test",
  typeId: "point",
  versionMajor: 1,
  versionMinor: 0,
};
const pointCodec: sdk.ContentCodec<Point> = {
  type: pointType,
  encode: (value) => ({
    type: pointType,
    parameters: new Map(),
    content: new TextEncoder().encode(JSON.stringify(value)),
  }),
  decode: (encoded) => JSON.parse(new TextDecoder().decode(encoded.content)),
};

// Options without a backend lower to an absent backend, so the binding uses
// its default connection options.
assert.equal(
  lowerClientOptions({ storage: { location: "inMemory" } }, currentProjection())
    .backend,
  undefined,
);

// Public errors from a generated wrapper, a membership guard, and a static.
const alice = await sdk.Client.create(signerFor(), await options([pointCodec]));
await assert.rejects(
  alice.conversations.getMessageById("AB".repeat(32)),
  publicError(sdk.XmtpError.InvalidArgument, "InvalidArgument", "input"),
);
const group = await alice.conversations.createGroup([]);
await assert.rejects(
  group.addMembers([alice.inboxId, alice.identity] as never),
  publicError(sdk.XmtpError.InvalidArgument, "InvalidArgument", "input"),
);
// A host static rethrows too; the code is the backend's, not this layer's.
await assert.rejects(
  sdk.Client.inboxStates(["not an inbox id"], backend),
  (error: unknown) =>
    error instanceof sdk.XmtpError &&
    typeof error.details.category === "string" &&
    !("tag" in error),
);

// Standalone codecs take and return public values.
const text = new sdk.TextCodec();
const encodedText = text.encode("hi");
assert.ok(encodedText.content instanceof Uint8Array);
assert.equal(encodedText.type.typeId, "text");
assert.equal(text.decode(encodedText), "hi");
assert.throws(
  () => new sdk.MarkdownCodec().decode(encodedText),
  publicError(sdk.XmtpError.InvalidArgument, "InvalidArgument", "input"),
);
const attachment: sdk.Attachment = {
  filename: "a.bin",
  mimeType: "application/octet-stream",
  content: new Uint8Array([1, 2, 3]),
};
const attachmentCodec = new sdk.AttachmentCodec();
const roundTrip = attachmentCodec.decode(attachmentCodec.encode(attachment));
assert.ok(roundTrip.content instanceof Uint8Array);
assert.deepEqual([...roundTrip.content], [1, 2, 3]);

// Streams yield public values. The next read acknowledges the prior message.
const states: (sdk.ConnectionState | undefined)[] = [];
const messages = sdk.MessageStream.openGroup(alice, group, undefined, {
  onConnectionStateChange: (previous, current) =>
    states.push(previous, current),
});
await messages.ready();
const sentId = await group.sendText("streamed");
const first = await messages.next();
assert.equal(first.done, false);
assert.ok(first.value instanceof sdk.Message);
assert.equal(first.value.id, sentId);
assert.deepEqual(first.value.content, { kind: "text", value: "streamed" });
assert.ok(first.value.deliveryCursor?.startsWith("dc1_"));
const customId = await group.send(pointCodec.encode({ x: 1, y: 2 }));
const custom = await messages.next();
assert.equal(custom.value?.id, customId);
assert.equal(custom.value?.content.kind, "custom");
assert.deepEqual(
  custom.value?.content.kind === "custom" && custom.value.content.value,
  { x: 1, y: 2 },
);
await messages.end();
// The app sees public state values, in order: the state at subscription
// first, with no previous state. The stream can connect before the SDK reads
// that state, so it is connecting or connected (PROC-044).
assert.equal(states[0], undefined);
assert.ok(
  states[1] === "connecting" || states[1] === "connected",
  `state at subscription ${states[1]}`,
);
const publicStates = new Set([
  undefined,
  "connecting",
  "connected",
  "reconnecting",
  "failed",
  "closed",
]);
for (const state of states)
  assert.ok(publicStates.has(state), `state ${state}`);

// A reconnect reaches the app as the ordered public states, like the host
// stream case in node-stream-lifecycle.mts, through the public options.
{
  const seen: [sdk.ConnectionState | undefined, sdk.ConnectionState][] = [];
  const changes: Array<(state: BoundState) => void> = [];
  const stream = new HostMessageStream(
    async () => ({
      next: () => new Promise<undefined>(() => undefined),
      end: async () => undefined,
      connectionState: async () => BoundState.Connected,
      connectionStateChanged: () =>
        new Promise<BoundState>((resolve) => changes.push(resolve)),
    }),
    alice,
    hostOptions({
      onConnectionStateChange: (previous, current) =>
        seen.push([previous, current]),
    }),
  );
  await stream.ready();
  // Wait with a deadline, so a regression fails instead of hanging.
  const until = async (ready: () => boolean, what: string) => {
    const deadline = Date.now() + 5_000;
    while (!ready()) {
      if (Date.now() > deadline) throw new Error(`timed out: ${what}`);
      await new Promise((r) => setTimeout(r, 1));
    }
  };
  for (const next of [BoundState.Reconnecting, BoundState.Connected]) {
    await until(() => changes.length > 0, "connection state request");
    changes.shift()!(next);
  }
  await until(() => seen.length >= 3, "three public connection states");
  assert.deepEqual(seen, [
    [undefined, "connected"],
    ["connected", "reconnecting"],
    ["reconnecting", "connected"],
  ]);
  await stream.end();
}

// The onValue path delivers the same public messages.
const replay = sdk.MessageStream.openGroup(alice, group, {
  from: await alice.conversations.beginningDeliveryCursor(),
});
const seen: sdk.Message[] = [];
await replay.onValue(async (message) => {
  seen.push(message);
  if (seen.length === 2) await replay.end();
});
assert.ok(seen.every((message) => message instanceof sdk.Message));
assert.deepEqual(seen[0]?.content, { kind: "text", value: "streamed" });

// A conversation stream yields the Group itself.
const conversations = sdk.ConversationStream.open(alice);
await conversations.ready();
const created = await alice.conversations.createGroup([]);
const joined = await conversations.next();
assert.ok(joined.value instanceof sdk.Group);
assert.equal(joined.value.id, created.id);
await conversations.end();

// Events and listeners receive public ClientEvent values.
const filter: sdk.EventFilter = {
  kinds: ["conversation.joined"],
  references_own_messages: false,
};
const events = await alice.events(filter);
const heard: sdk.ClientEvent[] = [];
let listened!: () => void;
const listenerHeard = new Promise<void>((resolve) => {
  listened = resolve;
});
const listener = await alice.startListener(filter, (event) => {
  heard.push(event);
  listened();
});
assert.equal(typeof listener, "bigint");
await alice.conversations.createGroup([]);
const event = await events.next();
assert.equal(event.done, false);
assert.equal(event.value?.kind, "conversation.joined");
assert.equal(
  event.value?.kind === "conversation.joined" &&
    typeof event.value.conversation_joined.origin,
  "string",
);
await listenerHeard;
assert.equal(heard[0]?.kind, "conversation.joined");
await alice.stopListener(listener);
await events.return();

// Logging: the sink receives public log records.
await sdk.initLogging({
  level: "error",
  structured: true,
  performance: false,
  otel: undefined,
  resourceAttributes: new Map(),
});
let logged!: (record: sdk.LogRecord) => void;
const record = new Promise<sdk.LogRecord>((resolve) => {
  logged = resolve;
});
await sdk.setLogSink({ log: async (entry) => { logged(entry); } });
await assert.rejects(
  sdk.localSignerFromPrivateKey(new Uint8Array(31)),
  (error: unknown) => error instanceof sdk.XmtpError,
);
const entry = await Promise.race([
  record,
  new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error("log sink did not run")), 3_000),
  ),
]);
await sdk.setLogSink(undefined);
assert.equal(typeof entry.level, "string");
assert.ok(entry.fields instanceof Map);

// An ended client's messages and open streams fail with the public
// ClientClosed error.
const open = sdk.MessageStream.openGroup(alice, group);
await open.ready();
await alice.end();
await assert.rejects(
  open.next(),
  publicError(sdk.XmtpError.ClientClosed, "ClientClosed", "lifecycle"),
);
assert.throws(
  () => first.value!.client(),
  publicError(sdk.XmtpError.ClientClosed, "ClientClosed", "lifecycle"),
);
await assert.rejects(
  first.value!.refresh(),
  publicError(sdk.XmtpError.ClientClosed, "ClientClosed", "lifecycle"),
);
console.log("Node public streams, events, codecs, and errors passed");
