import { waitForLog } from "../../../../apps/xmtp_sdk_bindgen/runtime-tests/ts/logging-wait.js";
// The browser private public entry in real Chromium, through the package
// worker: public objects over the worker proxies, `Group | Dm`, the host
// Message with public fields and actions, streams, events, public errors,
// worker-routed constructors and functions, and `Storage.admin()`.
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as pure from "../../../../target/sdk-generated/typescript-pure/index";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { RemoteObject } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/remote-object";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import { BridgeError } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import { boundMessage } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/message";

function check(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

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
      const signed = await account.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signed)) };
    },
  };
}

// A public value is plain data: no binding tag, payload, or class instance.
function isPublic(value: unknown, seen = new Set<unknown>()): boolean {
  if (value === null || typeof value !== "object" || seen.has(value))
    return true;
  seen.add(value);
  if (value instanceof ArrayBuffer) return false;
  if (value instanceof Uint8Array || value instanceof sdk.Timestamp)
    return true;
  const prototype: unknown = Object.getPrototypeOf(value);
  if (
    prototype !== Object.prototype &&
    prototype !== Array.prototype &&
    prototype !== Map.prototype &&
    prototype !== sdk.Message.prototype
  )
    return false;
  if ("tag" in value || "inner" in value) return false;
  const entries =
    value instanceof Map ? [...value.values()] : Object.values(value);
  return entries.every((item) => isPublic(item, seen));
}

const PROXY_MEMBERS = ["checkLive", "release", "handle", "session"];

// The public Message is a value class: its own properties are the fields of
// its MessageData but the client key, and its decoded content. The generator
// writes one per MessageData field, so the list comes from the binding data.
function messageFields(message: sdk.Message): ReadonlySet<string> {
  const data = Object.keys(boundMessage(message).data);
  check(data.includes("clientKey"), "message data has no client key");
  return new Set([
    ...data.filter((key) => key !== "clientKey"),
    "content",
    "inReplyToContent",
    "replyContent",
  ]);
}

// True when a bridge proxy or session is reachable through the properties of
// `value`, enumerable or not, including symbol keys.
function reachesTransport(value: unknown, seen = new Set<unknown>()): boolean {
  if (value === null || typeof value !== "object" || seen.has(value))
    return false;
  seen.add(value);
  if (value instanceof RemoteObject || value instanceof MainSession)
    return true;
  const keys: (string | symbol)[] = [
    ...Object.getOwnPropertyNames(value),
    ...Object.getOwnPropertySymbols(value),
  ];
  return keys.some((key) => {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    return descriptor !== undefined && "value" in descriptor
      ? reachesTransport(descriptor.value, seen)
      : false;
  });
}

// P7: a public object shows no transport. It has no own properties other than
// its documented public fields, no symbol keys, JSON shows only those fields,
// and no proxy member or bridge object is reachable from it.
function checkOpaque(
  name: string,
  value: object,
  fields: ReadonlySet<string> = new Set(),
): void {
  check(
    Object.getOwnPropertySymbols(value).length === 0,
    `${name} has symbol keys`,
  );
  const own = Object.getOwnPropertyNames(value).filter(
    (key) => !fields.has(key),
  );
  check(own.length === 0, `${name} has own properties: ${own.join(", ")}`);
  const json: unknown = JSON.parse(
    JSON.stringify(value, (_key, item: unknown) =>
      typeof item === "bigint" ? item.toString() : item,
    ),
  );
  check(
    json !== null &&
      typeof json === "object" &&
      Object.keys(json).every((key) => fields.has(key)),
    `${name} JSON shows more than its public fields`,
  );
  for (const member of PROXY_MEMBERS)
    check(!(member in value), `${name} exposes the proxy member ${member}`);
  check(!reachesTransport(value), `${name} reaches the transport`);
}

// Fail when any string in `value`, at any depth, carries the secret.
function assertNoSecret(
  value: unknown,
  secret: string,
  path = "options",
  seen = new Set<unknown>(),
): void {
  if (typeof value === "string") {
    check(!value.includes(secret), `${path} exposes a secret`);
    return;
  }
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  for (const key of Object.getOwnPropertyNames(value)) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (descriptor !== undefined && "value" in descriptor)
      assertNoSecret(descriptor.value, secret, `${path}.${key}`, seen);
  }
}

async function rejection(operation: Promise<unknown>): Promise<unknown> {
  try {
    await operation;
  } catch (error) {
    return error;
  }
  throw new Error("the operation did not fail");
}

// A public error, never a binding or bridge error.
function isPublicError(error: unknown): error is sdk.XmtpError {
  return (
    error instanceof sdk.XmtpError &&
    !(error instanceof BridgeError) &&
    !("tag" in error) &&
    !("inner" in error)
  );
}

export async function exercise(): Promise<string[]> {
  const results: string[] = [];
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const options: sdk.ClientOptions = {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  };
  const alice = await sdk.Client.create(signerFor(), options);
  const bob = await sdk.Client.create(signerFor(), options);
  try {
    // Public objects wrap the proxies and expose no transport member.
    const conversations = alice.conversations;
    check(conversations instanceof sdk.Conversations, "no Conversations");
    const group = await conversations.createGroup([bob.identity]);
    check(group instanceof sdk.Group, "createGroup did not return a Group");
    check(group.kind === "group", "Group kind is not a public literal");
    const fetched = await conversations.getById(group.id);
    check(fetched instanceof sdk.Group, "getById did not return the Group");
    const dm = await conversations.createDm(bob.inboxId);
    check(dm instanceof sdk.Dm, "createDm did not return a Dm");
    check((await dm.peerInboxId()) === bob.inboxId, "wrong DM peer");
    results.push("objects and Group | Dm");

    // The public options never return the backend token: the worker copies
    // the redacted Rust options to the page, and the projection lifts them.
    const token = `Bearer public-${crypto.randomUUID()}`;
    const credentialed = await sdk.Client.create(signerFor(), {
      ...options,
      backend: {
        ...backend,
        credentials: { value: token, expiresAtSeconds: 9_007_199_254_740_993n },
      },
    });
    try {
      const saved = credentialed.options.backend;
      check(
        saved !== undefined &&
          !(saved instanceof sdk.Backend) &&
          saved.credentials === undefined,
        "the public options return the backend credentials",
      );
      assertNoSecret(credentialed.options, token);
    } finally {
      await credentialed.end();
    }
    results.push("options without the backend token");

    // The host Message: public fields, actions, and the owner Client.
    const id = await group.sendText("public browser");
    const message = (await group.messages()).find((item) => item.id === id);
    check(message instanceof sdk.Message, "no public Message");
    check(message.content.kind === "text", "content is not a public union");
    check(message.deliveryStatus === "published", "status is not a literal");
    check(
      message.encoded.content instanceof Uint8Array,
      "bytes are not a view",
    );
    check(isPublic(message), "a Message field is not a public value");
    check(message.client() === alice, "Message did not return its Client");
    const replyId = await message.reply("reply");
    const reply = await conversations.getMessageById(replyId);
    check(reply?.content.kind === "reply", "reply is not a reply");
    check(reply.replyContent?.kind === "text", "reply body is not text");
    check((await reply.parent())?.id === id, "parent action failed");
    results.push("host Message fields and actions");

    // custom_codec_policy_and_isolation through the worker: the fallback hook
    // fills the nested envelope, and a typed send and prepare store the
    // codec's envelope. suite.chromium.ts checks per-client isolation.
    const noteType: sdk.ContentTypeId = {
      authorityId: "example.org",
      typeId: "note",
      versionMajor: 1,
      versionMinor: 0,
    };
    const noteCodec: sdk.ContentCodec<string> = {
      type: noteType,
      encode: (value) => ({
        type: noteType,
        parameters: new Map(),
        content: new TextEncoder().encode(value),
      }),
      decode: (encoded) => new TextDecoder().decode(encoded.content),
      fallback: (value) => `a note: ${value}`,
      shouldPush: () => {
        throw new Error("a reply does not call shouldPush");
      },
    };
    const noteId = await message.reply(noteCodec, "browser note");
    const note = (await group.messages()).find((item) => item.id === noteId);
    check(
      note?.content.kind === "reply" &&
        note.content.body.kind !== "text" &&
        "encoded" in note.content.body &&
        note.content.body.encoded.fallback === "a note: browser note",
      "the fallback hook did not fill the nested envelope",
    );
    // A typed send through the worker: the catalogue predicate runs on the
    // main thread in the pure module, and the hook's push reaches the send.
    const sentId = await group.send(
      { ...noteCodec, shouldPush: () => false },
      "browser send",
    );
    const sentNote = (await group.messages()).find(
      (item) => item.id === sentId,
    );
    check(
      sentNote?.content.kind === "unknown" &&
        sentNote.content.encoded.fallback === "a note: browser send",
      "a typed send did not store the codec's envelope",
    );
    const preparedNote = await group.prepareMessage(
      { ...noteCodec, shouldPush: () => true },
      "browser prepared",
    );
    check(
      (await group.messages()).some(
        (item) =>
          item.id === preparedNote && item.deliveryStatus === "unpublished",
      ),
      "a typed prepareMessage did not store an unpublished item",
    );
    await group.publishMessage(preparedNote);
    results.push("custom_codec_policy_and_isolation");

    // codec_policy_failure_never_publishes: a failed encode, fallback, or
    // shouldPush step makes no publish attempt, and a skipped hook is not
    // called.
    await group.send(
      {
        ...noteCodec,
        shouldPush: () => {
          throw new Error("an explicit shouldPush skips the hook");
        },
      },
      "explicit push",
      { shouldPush: false },
    );
    const beforeFailure = (await group.messages()).length;
    const failingEncode: sdk.ContentCodec<string> = {
      ...noteCodec,
      encode: () => {
        throw new Error("encode failed");
      },
    };
    const failedSteps: sdk.ContentCodec<string>[] = [
      failingEncode,
      {
        ...noteCodec,
        fallback: () => {
          throw new Error("fallback failed");
        },
      },
      {
        ...noteCodec,
        shouldPush: () => {
          throw new Error("shouldPush failed");
        },
      },
    ];
    for (const failing of failedSteps) {
      const failed = await rejection(group.send(failing, "never sent"));
      check(
        failed instanceof sdk.XmtpError.CodecEncodeFailed &&
          isPublicError(failed) &&
          failed.details.category === "callback",
        `a failed codec step is not CodecEncodeFailed: ${String(failed)}`,
      );
    }
    const failedEncode = await rejection(
      message.reply(failingEncode, "never sent"),
    );
    check(
      failedEncode instanceof sdk.XmtpError.CodecEncodeFailed &&
        isPublicError(failedEncode) &&
        failedEncode.details.category === "callback",
      `a failed encode is not CodecEncodeFailed: ${String(failedEncode)}`,
    );
    check(
      (await group.messages()).length === beforeFailure,
      "a failed codec step made a publish attempt",
    );
    results.push("codec_policy_failure_never_publishes");

    // Streams and events yield public values.
    const stream = group.streamMessages();
    const streamedId = await group.sendText("streamed");
    let streamed: sdk.Message | undefined;
    for (;;) {
      const item = await stream.next();
      check(!item.done, "the stream ended early");
      if (item.value.id === streamedId) {
        streamed = item.value;
        break;
      }
    }
    await stream.end();
    check(streamed instanceof sdk.Message, "the stream yielded no Message");
    const joined = alice.conversations.stream();
    await joined.ready();
    const created = await conversations.createGroup([]);
    const next = await joined.next();
    check(next.value instanceof sdk.Group, "no Group from the stream");
    check(next.value.id === created.id, "wrong joined Group");
    await joined.end();
    const events = await alice.events({
      kinds: ["conversation.joined"],
      references_own_messages: false,
    });
    await conversations.createGroup([]);
    const event = await events.next();
    check(event.value?.kind === "conversation.joined", "no public event");
    await events.return();
    results.push("streams and events");

    // P7 over every public object kind.
    const messageReader = await conversations.messageReader();
    const conversationReader =
      await conversations.conversationReader(undefined);
    const openStream = group.streamMessages();
    const openJoined = alice.conversations.stream();
    const openEvents = await alice.events({
      kinds: ["conversation.joined"],
      references_own_messages: false,
    });
    const opaqueBackend = await sdk.Backend.connect(backend);
    const opaqueAdmin = await sdk.Storage.admin();
    const kinds: [string, object][] = [
      ["Client", alice],
      ["Conversations", conversations],
      ["Preferences", alice.preferences],
      ["Diagnostics", alice.diagnostics],
      ["Archives", alice.archives],
      ["Storage", alice.storage],
      ["Group", group],
      ["Dm", dm],
      ["MessageReader", messageReader],
      ["ConversationReader", conversationReader],
      ["MessageStream", openStream],
      ["ConversationStream", openJoined],
      ["EventStream", openEvents],
      ["Backend", opaqueBackend],
      ["StorageAdmin", opaqueAdmin],
    ];
    for (const [name, value] of kinds) checkOpaque(name, value);
    checkOpaque("Message", message, messageFields(message));
    check(
      message.rawBytes instanceof Uint8Array && message.rawBytes.length > 0,
      "public message bytes missing",
    );
    await messageReader.end();
    await conversationReader.end();
    await openStream.end();
    await openJoined.end();
    await openEvents.return();
    await opaqueAdmin.end();
    results.push("no transport on any public object");

    // Public errors and worker-routed constructors and functions.
    let invalid: unknown;
    try {
      await conversations.getMessageById("AB".repeat(32));
    } catch (error) {
      invalid = error;
    }
    check(
      invalid instanceof sdk.XmtpError.InvalidArgument &&
        invalid.details.category === "input" &&
        !("tag" in invalid),
      "a malformed ID did not fail with the public error",
    );
    // The pure module shares the package's error and Timestamp classes.
    await pure.initPureWasm();
    check(pure.XmtpError === sdk.XmtpError, "two XmtpError classes");
    check(pure.Timestamp === sdk.Timestamp, "two Timestamp classes");
    check(message.sentAt instanceof pure.Timestamp, "sentAt is another class");
    const pureError = await rejection(
      Promise.resolve().then(() =>
        pure.decodeStandard({
          ...message.encoded,
          content: new Uint8Array([0xff]),
        }),
      ),
    );
    check(
      pureError instanceof sdk.XmtpError && isPublicError(pureError),
      "a pure module error is not the package XmtpError",
    );
    const connected = await sdk.Backend.connect(backend);
    check(connected instanceof sdk.Backend, "Backend.connect failed");
    check(
      (await sdk.Client.inboxIdFor(alice.identity, connected)) ===
        alice.inboxId,
      "a static call with a connected Backend failed",
    );
    // setLogSink returns a Promise on both targets; here it runs in the
    // package worker. Before initLogging it rejects with the public
    // InvalidInput, as on Node.
    const cleared = sdk.setLogSink();
    check(cleared instanceof Promise, "setLogSink did not return a Promise");
    const clearError = await rejection(cleared);
    check(
      clearError instanceof sdk.XmtpError.InvalidInput &&
        isPublicError(clearError),
      `setLogSink before initLogging: ${String(clearError)}`,
    );
    const configuration = await sdk.fetchServerConfiguration(backend);
    check(isPublic(configuration), "configuration is not a public value");
    results.push("public errors, Backend.connect, and worker functions");

    // Storage admin opens without a Client, and its failures are public.
    const admin = await sdk.Storage.admin();
    check(typeof (await admin.poolCapacity()) === "number", "no capacity");
    const badPath = await rejection(admin.deleteFile("file:bad.db3"));
    check(
      badPath instanceof sdk.XmtpError.InvalidInput && isPublicError(badPath),
      "a SQLite URI path did not fail with the public InvalidInput",
    );
    const badBytes = await rejection(
      admin.importDb(
        `bad-${crypto.randomUUID()}.db3`,
        new Uint8Array([1, 2, 3]),
      ),
    );
    // The typed storage cause names the code and its retry policy.
    check(
      badBytes instanceof sdk.XmtpError.Storage && isPublicError(badBytes),
      `bad import bytes did not fail with the public Storage: ${String(badBytes)}`,
    );
    await admin.end();
    const endedAdmin = await rejection(admin.fileCount());
    check(
      endedAdmin instanceof sdk.XmtpError.ClientClosed &&
        isPublicError(endedAdmin),
      "an ended admin did not fail with the public ClientClosed",
    );
    results.push("Storage.admin and its public errors");

    const aliceInboxId = alice.inboxId;
    const groupId = group.id;
    const groupTopic = group.topic;
    await alice.end();
    let closed: unknown;
    try {
      message.client();
    } catch (error) {
      closed = error;
    }
    check(
      closed instanceof sdk.XmtpError.ClientClosed,
      "an ended Client's Message did not fail with ClientClosed",
    );
    // Getters read held values, so they stay readable after end, as on Node
    // (Decision 14). Calls fail with the public ClientClosed.
    check(alice.inboxId === aliceInboxId, "inboxId changed after end");
    check(
      alice.conversations instanceof sdk.Conversations,
      "conversations failed after end",
    );
    check(group.id === groupId, "Group id changed after end");
    check(group.topic === groupTopic, "Group topic changed after end");
    const endedCall = await rejection(group.sync());
    check(
      endedCall instanceof sdk.XmtpError.ClientClosed &&
        isPublicError(endedCall),
      "a call after end did not fail with the public ClientClosed",
    );
    results.push("ClientClosed after end, and getters stay readable");

    // A collected Client also closes its messages' actions: the Message
    // holds its Client weakly.
    const orphan = await orphanMessage(options);
    const collect: unknown = Reflect.get(globalThis, "gc");
    check(typeof collect === "function", "gc is not exposed");
    let orphaned = false;
    for (let attempt = 0; attempt < 100 && !orphaned; attempt++) {
      collect();
      await new Promise((resolve) => setTimeout(resolve, 20));
      try {
        orphan.client();
      } catch (error) {
        check(
          error instanceof sdk.XmtpError.ClientClosed,
          "a collected Client's Message failed with another error",
        );
        orphaned = true;
      }
    }
    check(orphaned, "a Message kept its collected Client alive");
    results.push("ClientClosed after the Client is collected");
  } finally {
    await alice.end().catch(() => undefined);
    await bob.end();
  }
  return results;
}

// Only the Message leaves this function, so the Client can be collected.
async function orphanMessage(options: sdk.ClientOptions): Promise<sdk.Message> {
  const owner = await sdk.Client.create(signerFor(), options);
  const group = await owner.conversations.createGroup([]);
  const id = await group.sendText("orphan");
  const found = (await group.messages()).find((item) => item.id === id);
  check(found !== undefined, "no orphan message");
  return found;
}

/**
 * A package worker failure reaches the app as a public error from each
 * worker-routed function, constructor, and the storage admin.
 */
export async function failures(
  failWorkers: (on: boolean) => void,
): Promise<string[]> {
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const calls: [string, () => Promise<unknown>][] = [
    ["generateLocalSigner", () => sdk.generateLocalSigner()],
    ["flushTelemetry", () => sdk.flushTelemetry()],
    ["fetchServerConfiguration", () => sdk.fetchServerConfiguration(backend)],
    ["Backend.connect", () => sdk.Backend.connect(backend)],
    ["Storage.admin", () => sdk.Storage.admin()],
    [
      "Client.create",
      () =>
        sdk.Client.create(signerFor(), {
          backend,
          storage: { location: "inMemory" },
          deviceSync: false,
        }),
    ],
  ];
  const results: string[] = [];
  failWorkers(true);
  try {
    for (const [name, call] of calls) {
      const error = await rejection(call());
      check(
        isPublicError(error) && error.details.category === "lifecycle",
        `${name} did not fail with a public lifecycle error: ${String(error)}`,
      );
      results.push(name);
    }
  } finally {
    failWorkers(false);
  }
  return results;
}

/**
 * Getters after the package worker retired: the only Client ends, nothing
 * else keeps the worker, and the worker terminates. Every getter still reads
 * its held value, as on Node (Decision 14). The nested objects are read for
 * the first time here, and a call through one fails with ClientClosed.
 */
export async function retiredWorker(
  terminated: () => number,
  waitForTermination: (count: number) => Promise<void>,
): Promise<string[]> {
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const before = terminated();
  const client = await sdk.Client.create(signerFor(), {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  const inboxId = client.inboxId;
  const installationId = client.installationId;
  const group = await client.conversations.createGroup([]);
  const groupId = group.id;
  const groupTopic = group.topic;
  await client.end();
  await waitForTermination(before + 1);
  check(client.inboxId === inboxId, "inboxId failed after the worker retired");
  check(
    client.installationId === installationId,
    "installationId failed after the worker retired",
  );
  check(
    client.options.deviceSync === false,
    "options failed after the worker retired",
  );
  check(isPublic(client.identity), "identity failed after the worker retired");
  const objects: [string, () => object, unknown][] = [
    ["conversations", () => client.conversations, sdk.Conversations],
    ["preferences", () => client.preferences, sdk.Preferences],
    ["diagnostics", () => client.diagnostics, sdk.Diagnostics],
    ["archives", () => client.archives, sdk.Archives],
    ["storage", () => client.storage, sdk.Storage],
  ];
  for (const [name, read, type] of objects) {
    const value = read();
    check(
      typeof type === "function" && value instanceof type,
      `${name} failed after the worker retired`,
    );
  }
  check(group.id === groupId, "Group id failed after the worker retired");
  check(
    group.topic === groupTopic,
    "Group topic failed after the worker retired",
  );
  const call = await rejection(client.conversations.sync());
  check(
    call instanceof sdk.XmtpError.ClientClosed && isPublicError(call),
    "a call after the worker retired did not fail with ClientClosed",
  );
  return ["getters after the worker retired", "ClientClosed calls"];
}

/**
 * A foreign Restored DM and group through the package root. C imports A's
 * archive and is neither DM member, so the DM has no peer for C: the public
 * result is null, never undefined (Decision 13). Restored creators are also
 * unknown until activation. The independent adder stays equal to A's inbox.
 */
export async function restored(): Promise<string[]> {
  const backend: sdk.BackendOptions = { url: `${location.origin}/backend` };
  const options: sdk.ClientOptions = {
    backend,
    storage: { location: "inMemory" },
    deviceSync: false,
  };
  const a = await sdk.Client.create(signerFor(), options);
  const b = await sdk.Client.create(signerFor(), options);
  const c = await sdk.Client.create(signerFor(), options);
  try {
    const dm = await a.conversations.createDm(b.inboxId);
    check((await dm.peerInboxId()) === b.inboxId, "live DM peer");
    await dm.sendText("foreign restored DM");
    const group = await a.conversations.createGroup([]);
    await group.sendText("foreign restored group");
    const key = new Uint8Array(32).fill(9);
    const archive = await a.archives.exportToBytes(key, {
      elements: ["messages"],
    });
    await c.archives.importFromBytes(archive, key);
    const restoredDm = await c.conversations.getById(dm.id);
    check(restoredDm instanceof sdk.Dm, "restored DM missing");
    const peer = await restoredDm.peerInboxId();
    check(peer === null, `restored DM peer is ${String(peer)}, not null`);
    const listed = await c.conversations.listDms({ includeDuplicateDms: true });
    check(listed.length > 0, "no restored DM in the list");
    for (const item of listed) {
      const listedPeer = await item.peerInboxId();
      check(listedPeer === null, `listed DM peer is ${String(listedPeer)}`);
    }
    const restoredGroup = await c.conversations.getById(group.id);
    check(restoredGroup instanceof sdk.Group, "restored group missing");
    const listedGroups = await c.conversations.listGroups(undefined);
    const listedGroup = listedGroups.find((item) => item.id === group.id);
    check(listedGroup !== undefined, "restored group missing from list");
    const listedDm = listed.find((item) => item.id === dm.id);
    check(listedDm !== undefined, "restored DM missing from list");
    for (const [name, conversation] of [
      ["get group", restoredGroup],
      ["get DM", restoredDm],
      ["listed group", listedGroup],
      ["listed DM", listedDm],
    ] as const) {
      check(
        conversation.creatorInboxId === null,
        `${name} creator is ${String(conversation.creatorInboxId)}, not null`,
      );
      check(
        conversation.isCreator === false,
        `${name} isCreator is ${String(conversation.isCreator)}, not false`,
      );
      check(
        conversation.addedByInboxId === a.inboxId,
        `${name} adder is ${String(conversation.addedByInboxId)}, not the source inbox`,
      );
    }
    return [
      "restored DM peer and creator are null",
      "restored adder is preserved",
    ];
  } finally {
    await a.end();
    await b.end();
    await c.end();
  }
}

// verifies: LOG-008
export async function loggingEnd(): Promise<void> {
  const client = await sdk.Client.create(signerFor(), {
    backend: { url: `${location.origin}/backend` },
    storage: { location: "inMemory" },
    deviceSync: false,
  });
  await sdk.initLogging({ level: "error" });
  await loggingSecrets();
  let resolve!: () => void;
  let reject!: (error: unknown) => void;
  const ended = new Promise<void>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  await sdk.setLogSink({
    async log() {
      try {
        await client.end();
        await sdk.setLogSink();
        resolve();
      } catch (error) {
        reject(error);
      }
    },
  });
  await rejection(sdk.localSignerFromPrivateKey(new Uint8Array(31)));
  await waitForLog(ended, "browser log callback end did not complete");
  const error = await rejection(client.isRegistered());
  check(
    error instanceof sdk.XmtpError.ClientClosed,
    "log callback left the client open",
  );
}

// verifies: LOG-010
export async function loggingSecrets(): Promise<void> {
  const credential = "LOG_CREDENTIAL_SENTINEL_89d42";
  const signing = new TextEncoder().encode("LOG_SIGNING_KEY_SENTINEL_89d42!!!");
  const forbidden = [
    credential,
    new TextDecoder().decode(signing),
    [...signing].map((byte) => byte.toString(16).padStart(2, "0")).join(""),
    `[${[...signing].join(", ")}]`,
  ];
  for (const operation of [
    () =>
      sdk.Backend.connect({
        url: `${location.origin}/backend`,
        credentials: {
          value: `Bearer ${credential}\n`,
          expiresAtSeconds: 0n,
        },
      }),
    () => sdk.localSignerFromPrivateKey(signing),
  ]) {
    let delivered!: () => void;
    const received = new Promise<void>((resolve) => {
      delivered = resolve;
    });
    const logs: string[] = [];
    await sdk.setLogSink({
      async log(record) {
        logs.push(record.message, ...record.fields.values());
        delivered();
      },
    });
    await rejection(operation());
    await waitForLog(received, "browser secret log did not run");
    await sdk.setLogSink();
    check(
      !forbidden.some((secret) => logs.some((log) => log.includes(secret))),
      "browser app log exposed a secret",
    );
  }
}
