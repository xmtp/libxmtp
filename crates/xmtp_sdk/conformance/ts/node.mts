import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { setEventStartHookForTest } from "../../../../target/sdk-conformance/typescript-napi/runtime/client.ts";
import { assertEncodedEqual, checkStandardCodecs } from "./node-codecs.mts";
import { logging } from "./node-logging.mts";
import { readerDelivery } from "./node-reader-delivery.mts";
import { streamFailures } from "./node-stream-failures.mts";
import { streamLifecycle } from "./node-stream-lifecycle.mts";

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

assert.equal(typeof sdk.Client.create, "function");
assert.equal(typeof sdk.Message, "function");
assert.equal(typeof sdk.Timestamp, "function");
assert.throws(
  () => new sdk.MarkdownCodec().decode(sdk.encodeText("wrong codec")),
  sdk.XmtpError.InvalidArgument,
);
await sdk.uniffiInitAsync();
assert.throws(
  () => new sdk.ReadReceiptCodec().encode("wrong value" as never),
  sdk.XmtpError.InvalidArgument,
);
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
console.log("Node scenario 1: load, checksums, version passed");

const codecSamples = checkStandardCodecs();

const account = privateKeyToAccount(generatePrivateKey());
const identity = {
  identifier: account.address.toLowerCase(),
  kind: sdk.PublicIdentityKind.Ethereum,
};
const signer = {
  async identity() {
    return identity;
  },
  async kind() {
    return new sdk.SignerKind.Eoa();
  },
  async sign(request: { text: string }) {
    const signature = await account.signMessage({ message: request.text });
    return new sdk.Signature.Ecdsa(Uint8Array.from(toBytes(signature)).buffer);
  },
};
const backendOptions = {
  url: process.env.XMTP_BACKEND_URL!,
  appVersion: undefined,
  credentials: undefined,
  credential: undefined,
};
const options = {
  backend: new sdk.BackendSource.Options({ options: backendOptions }),
  storage: {
    location: new sdk.StorageLocation.Directory(
      await mkdtemp(join(tmpdir(), "xmtp-sdk-conformance-")),
    ),
    label: undefined,
    encryptionKey: undefined,
    pool: undefined,
    singleConnection: false,
  },
  deviceSync: false,
  registration: { auto: true, nonce: undefined },
  forkRecovery: undefined,
  workers: undefined,
};
assert.equal(
  sdk.ClientOptions.create({ storage: options.storage }).backend,
  undefined,
);

const client = await sdk.Client.create(signer, options);
await assert.rejects(
  client.conversations().getMessageById("bad"),
  sdk.XmtpError.InvalidArgument,
);
const inboxId = client.inboxId();
assert.equal(typeof inboxId.toString(), "string");
const storagePath = await client.storage().path();
assert.ok(storagePath);
assert.ok((await stat(storagePath)).isFile());
const group = await client.conversations().createGroup([], undefined);
let typedSends = 0;
for (const sample of codecSamples) {
  const value = sample.value;
  let id: sdk.MessageId;
  switch (value.tag) {
    case sdk.StandardContent_Tags.Text:
      id = await group.sendText(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.Markdown:
      id = await group.sendMarkdown(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.Reaction:
      id = await group.sendReaction(
        value.inner.reference,
        value.inner.referenceInboxId,
        value.inner.reaction,
        undefined,
      );
      break;
    case sdk.StandardContent_Tags.Reply:
      id = await group.sendReply(
        value.inner.reference,
        value.inner.referenceInboxId,
        value.inner.content,
        undefined,
      );
      break;
    case sdk.StandardContent_Tags.ReadReceipt:
      id = await group.sendReadReceipt(undefined);
      break;
    case sdk.StandardContent_Tags.Attachment:
      id = await group.sendAttachment(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.RemoteAttachment:
      id = await group.sendRemoteAttachment(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.MultiRemoteAttachment:
      id = await group.sendMultiRemoteAttachment(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.TransactionReference:
      id = await group.sendTransactionReference(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.WalletSendCalls:
      id = await group.sendWalletSendCalls(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.Actions:
      id = await group.sendActions(value.inner[0], undefined);
      break;
    case sdk.StandardContent_Tags.Intent:
      id = await group.sendIntent(value.inner[0], undefined);
      break;
    default:
      continue;
  }
  const wire = await client.conversations().getMessageById(id);
  assert.ok(wire);
  assertEncodedEqual(wire.encoded, sample.expected);
  typedSends++;
}
assert.equal(typedSends, 12);
console.log("Node P69: typed send bytes match all 12 public codecs");
const sentId = await group.sendText("conformance message", undefined);
const history = await group.messages(undefined);
const sent = history.find(
  (message) => message.id.toString() === sentId.toString(),
);
assert.ok(sent instanceof sdk.Message);
assert.equal(sent.client(), client);
await client.end();
assert.throws(
  () => sent.client(),
  (error) => error instanceof sdk.XmtpError.ClientClosed,
);

const reopened = await sdk.Client.build(identity, options, inboxId);
assert.equal(reopened.inboxId().toString(), inboxId.toString());
const defaultRoot = await mkdtemp(join(tmpdir(), "xmtp-sdk-default-"));
const oldCwd = process.cwd();
process.chdir(defaultRoot);
try {
  await assert.rejects(
    sdk.Client.build(
      identity,
      {
        ...options,
        storage: {
          ...options.storage,
          location: new sdk.StorageLocation.Default(),
        },
      },
      inboxId,
    ),
    (error) => error instanceof sdk.XmtpError.IdentityNotFound,
  );
  assert.equal((await readdir(join(defaultRoot, "xmtp"))).length, 0);
  // verifies: STORE-004
  const defaultClient = await sdk.Client.create(signer, {
    ...options,
    storage: {
      ...options.storage,
      location: new sdk.StorageLocation.Default(),
    },
  });
  const defaultPath = join(
    defaultRoot,
    "xmtp",
    `xmtp-${defaultClient.inboxId().toString()}.db3`,
  );
  assert.equal(await defaultClient.storage().path(), realpathSync(defaultPath));
  assert.ok((await stat(defaultPath)).isFile());
  await defaultClient.end();
} finally {
  process.chdir(oldCwd);
}
let releasedMessage: sdk.Message;
const weak = await (async () => {
  const shortLived = await sdk.Client.build(identity, options, inboxId);
  const shortGroup = await shortLived
    .conversations()
    .createGroup([], undefined);
  const id = await shortGroup.sendText("weak owner", undefined);
  releasedMessage = (await shortGroup.messages(undefined)).find(
    (value) => value.id.toString() === id.toString(),
  )!;
  return new WeakRef(shortLived);
})();
for (let i = 0; i < 30; i++) {
  await new Promise((resolve) => setTimeout(resolve, 20));
  global.gc?.();
  await new Promise((resolve) => setTimeout(resolve, 20));
  if (weak.deref() === undefined) break;
}
assert.equal(
  weak.deref(),
  undefined,
  "the registry kept the host client alive",
);
assert.throws(
  () => releasedMessage.client(),
  (error) => error instanceof sdk.XmtpError.ClientClosed,
);
console.log("Node scenario 2: create, reopen, end passed");

const delivery = await readerDelivery(reopened);
await streamLifecycle(reopened);
await streamFailures(reopened, delivery);

const largeExpiry = 9_007_199_254_740_993n;
const credentialOptions = {
  ...options,
  backend: new sdk.BackendSource.Options({
    options: {
      ...backendOptions,
      credential: {
        name: undefined,
        value: "Bearer initial",
        expiresAtSeconds: largeExpiry,
      },
    },
  }),
  storage: options.storage,
};
const credentialClient = await sdk.Client.build(
  identity,
  credentialOptions,
  inboxId,
);
const savedBackend = credentialClient.raw.options().backend;
assert.ok(savedBackend instanceof sdk.BackendSource.Options);
assert.equal(
  savedBackend.inner.options.credential?.expiresAtSeconds,
  largeExpiry,
);
await credentialClient.raw.setCredential({
  name: undefined,
  value: "Bearer refreshed",
  expiresAtSeconds: largeExpiry,
});
await credentialClient.end();
let sourceCalls = 0;
const sourceClient = await sdk.Client.build(
  identity,
  {
    ...credentialOptions,
    backend: new sdk.BackendSource.Options({
      options: {
        ...backendOptions,
        credentials: {
          async credential() {
            sourceCalls += 1;
            return {
              name: undefined,
              value: "Bearer source",
              expiresAtSeconds: largeExpiry,
            };
          },
        },
      },
    }),
  },
  inboxId,
);
assert.ok(sourceCalls > 0, "credential source was not called");
await sourceClient.end();
console.log("Node scenario 3: credential update and 64-bit value passed");

const snapshot = reopened.raw.serverConfiguration();
const fetched = await sdk.fetchServerConfiguration(
  new sdk.BackendSource.Options({ options: backendOptions }),
);
assert.equal(snapshot.identifier, fetched.identifier);
const staticBackend = await sdk.Backend.connect(backendOptions);
assert.equal(
  (
    await sdk.Client.inboxIdFor(
      identity,
      new sdk.BackendSource.Connected({ backend: staticBackend }),
    )
  ).toString(),
  inboxId.toString(),
);
assert.equal(
  (
    await sdk.Client.canMessage(
      [identity],
      new sdk.BackendSource.Connected({ backend: staticBackend }),
    )
  ).get(identity.identifier),
  true,
);
assert.equal(
  (
    await sdk.Client.canMessage(
      [identity],
      new sdk.BackendSource.Options({ options: backendOptions }),
    )
  ).get(identity.identifier),
  true,
);
await assert.rejects(
  sdk.Client.build(
    identity,
    {
      ...options,
      backend: new sdk.BackendSource.Connected({ backend: staticBackend }),
      storage: {
        ...options.storage,
        location: new sdk.StorageLocation.InMemory(),
      },
    },
    inboxId,
  ),
  (error) => error instanceof sdk.XmtpError.IdentityNotFound,
);
assert.equal(
  (await reopened.raw.refreshServerConfiguration()).identifier,
  snapshot.identifier,
);
await assert.rejects(
  sdk.fetchServerConfiguration(
    new sdk.BackendSource.Options({
      options: {
        ...backendOptions,
        url: "http://127.0.0.1:1",
      },
    }),
  ),
  (error) => error instanceof sdk.XmtpError.ConfigurationUnavailable,
);
console.log("Node scenario 10: configuration and typed error passed");

await logging(reopened, snapshot);
const local = await sdk.generateLocalSigner();
await assert.rejects(
  sdk.localSignerFromPrivateKey(new Uint8Array(31).buffer),
  (error) => error instanceof sdk.XmtpError.InvalidInput,
);
const unsigned = await sdk.Client.create(local, {
  ...options,
  storage: { ...options.storage, location: new sdk.StorageLocation.InMemory() },
  registration: { auto: false, nonce: undefined },
});
assert.equal(await unsigned.raw.isRegistered(), false);
const request = await unsigned.raw.unsafeCreateInboxSignatureRequest();
assert.ok(request);
assert.ok((await request.signatureText()).length > 0);
await request.sign(local);
await unsigned.raw.unsafeApplySignatureRequest(request);
assert.equal(await unsigned.raw.isRegistered(), true);
await unsigned.end();
console.log("Node scenario 11: local signer and signature request passed");

// verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
function recordingSigner(calls: string[]) {
  const wallet = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return {
        identifier: wallet.address.toLowerCase(),
        kind: sdk.PublicIdentityKind.Ethereum,
      };
    },
    async kind() {
      return new sdk.SignerKind.Eoa();
    },
    async sign(request: { text: string }) {
      calls.push("sign");
      const signature = await wallet.signMessage({ message: request.text });
      return new sdk.Signature.Ecdsa(
        Uint8Array.from(toBytes(signature)).buffer,
      );
    },
  };
}
function preAuthenticateOptions(calls: string[], fail: boolean, auto: boolean) {
  return {
    ...options,
    storage: {
      ...options.storage,
      location: new sdk.StorageLocation.InMemory(),
    },
    registration: { auto, nonce: undefined },
    handlers: {
      preAuthenticate: {
        async run() {
          calls.push("pre-authenticate");
          if (fail) throw new sdk.PreAuthenticateError.Failed();
        },
      },
    },
  };
}
const preAuthCalls: string[] = [];
const preAuthenticated = await sdk.Client.create(
  recordingSigner(preAuthCalls),
  preAuthenticateOptions(preAuthCalls, false, false),
);
assert.deepEqual(preAuthCalls, []);
await preAuthenticated.raw.register();
assert.deepEqual(preAuthCalls, ["pre-authenticate", "sign"]);
preAuthCalls.length = 0;
await preAuthenticated.raw.register();
assert.deepEqual(preAuthCalls, []);
await preAuthenticated.end();
await assert.rejects(
  sdk.Client.create(
    recordingSigner(preAuthCalls),
    preAuthenticateOptions(preAuthCalls, true, true),
  ),
  (error) => error instanceof sdk.XmtpError.CallbackFailed,
);
assert.deepEqual(preAuthCalls, ["pre-authenticate"]);
console.log("Node IDENT-073: host preAuthenticate runs before the signer");

const familyGroup = await reopened.conversations().createGroup([], {
  permissions: undefined,
  name: "family group",
  imageUrl: undefined,
  description: undefined,
  disappearing: undefined,
  appData: undefined,
});
assert.equal((await familyGroup.state()).name, "family group");
assert.equal(familyGroup.creatorInboxId().toString(), inboxId.toString());
assert.ok(
  (await reopened.conversations().listGroups(undefined)).some(
    (value) => value.id().toString() === familyGroup.id().toString(),
  ),
);
console.log("Node scenario 4: group options, state, and list passed");

const parentId = await familyGroup.sendText("parent", undefined);
const reactionId = await reopened.conversations().reactToMessage(
  parentId,
  {
    content: "👍",
    action: sdk.ReactionAction.Added,
    schema: sdk.ReactionSchema.Unicode,
  },
  undefined,
);
const replyId = await reopened
  .conversations()
  .replyToMessage(parentId, sdk.encodeText("reply"), undefined);
assert.equal(
  (await reopened.raw.decodeContent(sdk.encodeText("decoded"))).tag,
  sdk.MessageContent_Tags.Text,
);
const familyMessages = await familyGroup.messages(undefined);
const parent = familyMessages.find(
  (value) => value.id.toString() === parentId.toString(),
);
const reply = familyMessages.find(
  (value) => value.id.toString() === replyId.toString(),
);
assert.equal(parent?.reactions[0]?.id.toString(), reactionId.toString());
assert.equal(parent?.replyCount, 1n);
assert.equal(reply?.inReplyTo?.id.toString(), parentId.toString());
const reactionMessage = await reopened
  .conversations()
  .getMessageById(reactionId);
if (reactionMessage?.content.tag !== sdk.MessageContent_Tags.Reaction)
  throw new Error("reaction message did not lift as a reaction");
assert.equal(
  reactionMessage.content.inner.reference.toString(),
  parentId.toString(),
);
assert.equal(
  reactionMessage.content.inner.referenceInboxId?.toString(),
  inboxId.toString(),
);
assert.equal(reactionMessage.content.inner.reaction.content, "👍");
console.log("Node scenario 5: message records, reaction, and reply passed");

const customType = sdk.ContentTypeId.create({
  authorityId: "example.org",
  typeId: "sample",
  versionMajor: 1,
  versionMinor: 0,
});
const customCodec = {
  type: customType,
  encode(value: string) {
    return sdk.EncodedContent.create({
      type: customType,
      content: new TextEncoder().encode(value).buffer,
    });
  },
  decode(value: sdk.EncodedContent) {
    return new TextDecoder().decode(value.content);
  },
};
const ownerWithCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [customCodec] },
  inboxId,
);
const ownerWithoutCodec = await sdk.Client.build(identity, options, inboxId);
const slashType = sdk.ContentTypeId.create({
  authorityId: "example.org",
  typeId: "a/b",
  versionMajor: 1,
  versionMinor: 0,
});
const slashCodec = {
  ...customCodec,
  type: slashType,
  encode(value: string) {
    return sdk.EncodedContent.create({
      type: slashType,
      content: new TextEncoder().encode(value).buffer,
    });
  },
  decode() {
    return "wrong codec";
  },
};
const slashHost = await sdk.Client.build(
  identity,
  { ...options, codecs: [slashCodec] },
  inboxId,
);
const colliding = sdk.EncodedContent.create({
  type: sdk.ContentTypeId.create({
    authorityId: "example.org/a",
    typeId: "b",
    versionMajor: 1,
    versionMinor: 0,
  }),
  content: new Uint8Array([1]).buffer,
});
assert.equal(
  slashHost.decodeCustom(colliding),
  undefined,
  "codec key collision selected the wrong codec",
);
const collidingMessage = new sdk.Message({
  clientKey: slashHost.raw.clientKey(),
  content: {
    tag: sdk.MessageContent_Tags.Custom,
    inner: { encoded: colliding, rawBytes: new ArrayBuffer(0) },
  },
  inReplyTo: undefined,
} as sdk.MessageData);
assert.equal(collidingMessage.content.tag, sdk.MessageContent_Tags.Unknown);
await slashHost.end();
const customId = await familyGroup.send(
  customCodec.encode("codec value"),
  undefined,
);
const decoded = await ownerWithCodec.conversations().getMessageById(customId);
const undecoded = await ownerWithoutCodec
  .conversations()
  .getMessageById(customId);
const customReplyId = await ownerWithCodec
  .conversations()
  .replyToMessage(customId, customCodec.encode("reply codec value"), undefined);
const customReply = await ownerWithCodec
  .conversations()
  .getMessageById(customReplyId);
const undecodedReply = await ownerWithoutCodec
  .conversations()
  .getMessageById(customReplyId);
assert.equal(undecoded?.content.tag, sdk.MessageContent_Tags.Unknown);
const serializedCustom = new Uint8Array([10, 3, 1, 2, 3]).buffer;
const syntheticUnknown = new sdk.Message({
  clientKey: ownerWithoutCodec.raw.clientKey(),
  content: {
    tag: sdk.MessageContent_Tags.Custom,
    inner: {
      encoded: customCodec.encode("codec value"),
      rawBytes: serializedCustom,
    },
  },
  inReplyTo: undefined,
} as sdk.MessageData);
if (syntheticUnknown.content.tag !== sdk.MessageContent_Tags.Unknown)
  throw new Error("synthetic content was not unknown");
assert.deepEqual(
  new Uint8Array(syntheticUnknown.content.inner.rawBytes),
  new Uint8Array(serializedCustom),
);
if (
  undecoded?.data.content.tag !== sdk.MessageContent_Tags.Custom ||
  undecoded.content.tag !== sdk.MessageContent_Tags.Unknown
)
  throw new Error("stored custom content was not unknown");
const rustRawBytes = undecoded.data.content.inner.rawBytes;
assert.ok(
  new Uint8Array(rustRawBytes).byteLength >
    new Uint8Array(undecoded!.encoded.content).byteLength,
);
assert.deepEqual(
  new Uint8Array(undecoded.content.inner.rawBytes),
  new Uint8Array(rustRawBytes),
);
assert.equal(undecodedReply?.replyContent?.tag, sdk.MessageBody_Tags.Unknown);
if (customReply?.replyContent?.tag !== sdk.MessageBody_Tags.Custom)
  throw new Error("custom reply was not decoded");
assert.equal(customReply.replyContent.inner.value, "reply codec value");
if (decoded?.content.tag !== sdk.MessageContent_Tags.Custom)
  throw new Error("custom message was not decoded");
assert.equal(decoded.content.inner.value, "codec value");
const failingCodec = {
  ...customCodec,
  decode(_value: sdk.EncodedContent): string {
    throw new Error("codec decode failed");
  },
};
const ownerWithFailingCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [failingCodec] },
  inboxId,
);
const failedDecode = await ownerWithFailingCodec
  .conversations()
  .getMessageById(customId);
if (failedDecode?.content.tag !== sdk.MessageContent_Tags.Custom)
  throw new Error("failed custom decode did not keep its content");
assert.match(String(failedDecode.content.inner.error), /codec decode failed/);
await ownerWithFailingCodec.end();
const throwingCodec = {
  ...customCodec,
  decode(_value: sdk.EncodedContent): string {
    throw new Error("codec exploded");
  },
};
const ownerWithThrowingCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [throwingCodec] },
  inboxId,
);
const throwingGroup = await ownerWithThrowingCodec
  .conversations()
  .createGroup([], undefined);
// verifies: PROC-045
const codecStream = new sdk.MessageStream(
  (signal) => throwingGroup.messageReader({ signal }),
  ownerWithThrowingCodec,
);
const brokenId = await throwingGroup.send(
  customCodec.encode("bad decode"),
  undefined,
);
const broken = (await codecStream.next()).value;
assert.equal(broken?.id.toString(), brokenId.toString());
assert.equal(broken?.content.tag, sdk.MessageContent_Tags.Custom);
assert.match(
  (broken?.content as { inner?: { error?: string } }).inner?.error ?? "",
  /codec exploded/,
);
const continuedId = await throwingGroup.sendText("after codec error");
assert.equal(
  (await codecStream.next()).value?.id.toString(),
  continuedId.toString(),
);
await codecStream.end();
await ownerWithThrowingCodec.end();
await ownerWithCodec.end();
await ownerWithoutCodec.end();
console.log("Node scenario 6: custom codec stayed with its client");

const archive = await reopened.raw
  .archives()
  .exportToBytes(new Uint8Array(32).fill(7).buffer, undefined);
assert.ok(archive.byteLength > 0);
assert.equal(
  (
    await reopened.raw
      .archives()
      .metadataFromBytes(archive, new Uint8Array(32).fill(7).buffer)
  ).backupVersion,
  0,
);
const archiveDir = await mkdtemp(join(tmpdir(), "xmtp-sdk-archive-"));
try {
  const archivePath = join(archiveDir, "snapshot.xmtp");
  await reopened.raw
    .archives()
    .exportToFile(archivePath, new Uint8Array(32).fill(7).buffer, undefined);
  assert.equal(
    (
      await reopened.raw
        .archives()
        .metadataFromFile(archivePath, new Uint8Array(32).fill(7).buffer)
    ).backupVersion,
    0,
  );
} finally {
  await rm(archiveDir, { recursive: true, force: true });
}
console.log("Node scenario 9: archive bytes and file passed");

// verifies: EVENT-014
// verifies: EVENT-050
// verifies: EVENT-053
const eventFilter = {
  kinds: [sdk.EventKind.ConversationJoined],
  conversationIds: undefined,
  contentTypes: undefined,
  referencesOwnMessages: false,
};
const eventReader = await reopened.raw.events(eventFilter);
let listenerCalls = 0;
const listenerId = await reopened.startListener(eventFilter, async () => {
  listenerCalls += 1;
});
await reopened.raw.conversations().createGroup([]);
const sampleEvent = await eventReader.next();
assert.ok(sampleEvent);
for (let attempt = 0; attempt < 100 && listenerCalls === 0; attempt += 1)
  await new Promise((resolve) => setTimeout(resolve, 10));
assert.equal(listenerCalls, 1);
await reopened.stopListener(listenerId);
await eventReader.end();
console.log("Node scenario 8: event reader and listener passed");

// verifies: EVENT-014
// verifies: EVENT-053
const eventStream = await reopened.events(eventFilter);
assert.ok(eventStream instanceof sdk.EventStream);
await reopened.raw.conversations().createGroup([]);
let publicEvents = 0;
for await (const event of eventStream) {
  assert.ok(event);
  publicEvents += 1;
  break;
}
assert.equal(publicEvents, 1, "public EventStream missed the event");
assert.deepEqual(await eventStream.next(), { done: true, value: undefined });
let endedReaders = 0;
const returnProbe = new sdk.EventStream({
  next: async () => sampleEvent,
  end: async () => {
    endedReaders += 1;
  },
});
for await (const _event of returnProbe) break;
assert.equal(endedReaders, 1, "EventStream.return did not end its reader");
console.log("Node public EventStream passed");

// verifies: EVENT-053
let releaseStart!: () => void;
let startArrived!: () => void;
const startHeld = new Promise<void>((resolve) => {
  releaseStart = resolve;
});
const startEntered = new Promise<void>((resolve) => {
  startArrived = resolve;
});
setEventStartHookForTest(async () => {
  startArrived();
  await startHeld;
});
let lateCalls = 0;
const delayedId = await reopened.startListener(eventFilter, () => {
  lateCalls += 1;
});
await reopened.raw.conversations().createGroup([]);
await startEntered;
await reopened.stopListener(delayedId);
releaseStart();
setEventStartHookForTest();
await new Promise((resolve) => setTimeout(resolve, 100));
assert.equal(lateCalls, 0, "callback started after stop returned");
console.log("Node delayed listener stop passed");

// verifies: EVENT-052
let resolveStopped!: () => void;
const stoppedInside = new Promise<void>((resolve) => {
  resolveStopped = resolve;
});
let reentrantId!: bigint;
reentrantId = await reopened.startListener(eventFilter, async () => {
  await reopened.stopListener(reentrantId);
  resolveStopped();
});
await reopened.raw.conversations().createGroup([]);
await Promise.race([
  stoppedInside,
  new Promise<never>((_, reject) =>
    setTimeout(
      () => reject(new Error("stop inside listener timed out")),
      10_000,
    ),
  ),
]);
console.log("Node stop from inside listener passed");

let resolveEnded!: () => void;
const endedInside = new Promise<void>((resolve) => {
  resolveEnded = resolve;
});
await reopened.startListener(eventFilter, async () => {
  await reopened.end();
  resolveEnded();
});
try {
  await reopened.raw.conversations().createGroup([]);
} catch {
  /* end may close this call */
}
await Promise.race([
  endedInside,
  new Promise<never>((_, reject) =>
    setTimeout(
      () => reject(new Error("end inside listener timed out")),
      10_000,
    ),
  ),
]);
console.log("Node end from inside listener passed");

await reopened.end();
console.log("Node scenario 7: durable stream and idle cancellation passed");
