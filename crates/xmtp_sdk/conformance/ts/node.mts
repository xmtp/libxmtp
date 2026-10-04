import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
// Host runtime internals that these checks drive directly: the event start
// hook and the host EventStream over a fake reader.
import { setEventStartHookForTest } from "../../../../target/sdk-conformance/typescript-napi/runtime/client.ts";
import { EventStream as HostEventStream } from "../../../../target/sdk-conformance/typescript-napi/runtime/events/reader.ts";
import { checkConfigurationMismatch } from "./config-mismatch.mts";
import { checkIdentityRoutes } from "./identity-routes.mts";
import { checkOnValueFailure } from "./node-callback-failure.mts";
import {
  attachmentEnd,
  attachmentFailures,
  attachmentFlow,
  attachmentRecords,
  attachmentSettings,
} from "./node-attachments.mts";
import { checkOnValueFailure } from "./node-callback-failure.mts";
import {
  assertEncodedEqual,
  checkStandardCodecs,
  isInvalidId,
} from "./node-codecs.mts";
import { logging } from "./node-logging.mts";
import { metadataFields } from "./node-metadata.mts";
import { readerDelivery } from "./node-reader-delivery.mts";
import { storageLayout } from "./node-storage-layout.mts";
import { streamFailures } from "./node-stream-failures.mts";
import { streamLifecycle } from "./node-stream-lifecycle.mts";
import { deploymentComponent } from "./node-support.mts";
import { checkReaderCursor, checkRestoredPeer } from "./reader-cursor.mts";

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
assert.throws(
  () => new sdk.ReadReceiptCodec().encode("wrong value" as never),
  sdk.XmtpError.InvalidArgument,
);
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
console.log("Node scenario 1: load, checksums, version passed");

const codecSamples = checkStandardCodecs();

const account = privateKeyToAccount(generatePrivateKey());
const identity: sdk.PublicIdentity = {
  identifier: account.address.toLowerCase(),
  kind: "ethereum",
};
const signer: sdk.Signer = {
  async identity() {
    return identity;
  },
  async kind() {
    return { kind: "eoa" };
  },
  async sign(request) {
    const signature = await account.signMessage({ message: request.text });
    return { kind: "ecdsa", value: Uint8Array.from(toBytes(signature)) };
  },
};
/** Fail when any string or byte array in `value` carries a secret. */
function assertNoSecret(
  value: unknown,
  secrets: (string | Uint8Array)[],
  path = "options",
  seen = new Set<object>(),
): void {
  if (typeof value === "string") {
    for (const secret of secrets)
      if (typeof secret === "string" && value.includes(secret))
        throw new Error(`${path} exposes a secret`);
    return;
  }
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) {
    const bytes = Buffer.from(
      value instanceof ArrayBuffer
        ? new Uint8Array(value)
        : new Uint8Array(value.buffer, value.byteOffset, value.byteLength),
    );
    for (const secret of secrets)
      if (secret instanceof Uint8Array && bytes.equals(Buffer.from(secret)))
        throw new Error(`${path} exposes a secret key`);
    return;
  }
  for (const key of Object.getOwnPropertyNames(value))
    assertNoSecret(
      (value as Record<string, unknown>)[key],
      secrets,
      `${path}.${key}`,
      seen,
    );
}

const backendOptions: sdk.BackendOptions = {
  url: process.env.XMTP_BACKEND_URL!,
};
const options = {
  backend: backendOptions,
  storage: {
    location: {
      directory: await mkdtemp(join(tmpdir(), "xmtp-sdk-conformance-")),
    },
    singleConnection: false,
  },
  deviceSync: false,
  registration: { auto: true },
} satisfies sdk.ClientOptions;

await checkReaderCursor(signer, backendOptions);
await checkRestoredPeer(backendOptions);
await checkIdentityRoutes(backendOptions);
await storageLayout(backendOptions);
await attachmentSettings(backendOptions);
await attachmentFlow(backendOptions);
await attachmentFailures(backendOptions);
await attachmentRecords(backendOptions);
await attachmentEnd(backendOptions);
await checkConfigurationMismatch(signer, backendOptions);
console.log(
  "Node offline build on another deployment fails with BackendMismatch",
);
await checkOnValueFailure(signer, backendOptions);
console.log("Node on_value_failure_is_failed_and_unacked passed");
const client = await sdk.Client.create(signer, options);
// Uppercase hex decodes, so only ID validation rejects it.
await assert.rejects(
  client.conversations.getMessageById("AB".repeat(32)),
  isInvalidId,
);
const inboxId = client.inboxId;
assert.equal(typeof inboxId, "string");
const storagePath = await client.storage.path();
assert.ok(storagePath);
assert.ok((await stat(storagePath)).isFile());
const group = await client.conversations.createGroup([]);
let typedSends = 0;
for (const sample of codecSamples) {
  const value = sample.value;
  let id: sdk.MessageId;
  switch (value.kind) {
    case "text":
      id = await group.sendText(value.value);
      break;
    case "markdown":
      id = await group.sendMarkdown(value.value);
      break;
    case "reaction":
      id = await group.sendReaction(
        value.reference,
        value.referenceInboxId,
        value.reaction,
      );
      break;
    case "reply":
      id = await group.sendReply(
        value.reference,
        value.referenceInboxId,
        value.content,
      );
      break;
    case "readReceipt":
      id = await group.sendReadReceipt();
      break;
    case "attachment":
      id = await group.sendAttachment(value.value);
      break;
    case "remoteAttachment":
      id = await group.sendRemoteAttachment(value.value);
      break;
    case "multiRemoteAttachment":
      id = await group.sendMultiRemoteAttachment(value.value);
      break;
    case "transactionReference":
      id = await group.sendTransactionReference(value.value);
      break;
    case "walletSendCalls":
      id = await group.sendWalletSendCalls(value.value);
      break;
    case "actions":
      id = await group.sendActions(value.value);
      break;
    case "intent":
      id = await group.sendIntent(value.value);
      break;
    default:
      continue;
  }
  const wire = await client.conversations.getMessageById(id);
  assert.ok(wire);
  assertEncodedEqual(wire.encoded, sample.expected);
  typedSends++;
}
assert.equal(typedSends, 12);
console.log("Node P69: typed send bytes match all 12 public codecs");
const sentId = await group.sendText("conformance message");
const history = await group.messages();
const sent = history.find((message) => message.id === sentId);
assert.ok(sent instanceof sdk.Message);
assert.equal(sent.client(), client);
await client.end();
assert.throws(
  () => sent.client(),
  (error) => error instanceof sdk.XmtpError.ClientClosed,
);

const reopened = await sdk.Client.build(identity, options, inboxId);
assert.equal(reopened.inboxId, inboxId);
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
          location: "default",
        },
      },
      inboxId,
    ),
    (error) => error instanceof sdk.XmtpError.IdentityNotFound,
  );
  const defaultFiles = await readdir(join(defaultRoot, "xmtp"), {
    recursive: true,
  }).catch(() => []);
  assert.ok(!defaultFiles.some((file) => file.endsWith(".db3")));
  // verifies: STORE-004
  const defaultClient = await sdk.Client.create(signer, {
    ...options,
    storage: {
      ...options.storage,
      location: "default",
    },
  });
  const defaultPath = join(
    defaultRoot,
    "xmtp",
    deploymentComponent(defaultClient.serverConfiguration.identifier),
    String(defaultClient.inboxId),
    "xmtp.db3",
  );
  assert.equal(await defaultClient.storage.path(), realpathSync(defaultPath));
  assert.ok((await stat(defaultPath)).isFile());
  await defaultClient.end();
} finally {
  process.chdir(oldCwd);
}
let releasedMessage: sdk.Message;
const weak = await (async () => {
  const shortLived = await sdk.Client.build(identity, options, inboxId);
  const shortGroup = await shortLived.conversations.createGroup([]);
  const id = await shortGroup.sendText("weak owner");
  releasedMessage = (await shortGroup.messages()).find(
    (value) => value.id === id,
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
  backend: {
    ...backendOptions,
    credentials: {
      name: undefined,
      value: "Bearer initial",
      expiresAtSeconds: largeExpiry,
    },
  },
  storage: options.storage,
  workers: { defaultIntervalNs: largeExpiry },
} satisfies sdk.ClientOptions;
const credentialClient = await sdk.Client.build(
  identity,
  credentialOptions,
  inboxId,
);
// The options keep 64-bit values but never return the backend token.
assert.equal(
  credentialClient.options.workers?.defaultIntervalNs,
  largeExpiry,
  "worker interval lost 64-bit precision",
);
const savedBackend = credentialClient.options.backend;
assert.ok(savedBackend !== undefined && !(savedBackend instanceof sdk.Backend));
assert.equal(savedBackend.credentials, undefined);
assertNoSecret(credentialClient.options, ["Bearer initial"]);
await credentialClient.setCredential({
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
    backend: {
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
  },
  inboxId,
);
assert.ok(sourceCalls > 0, "credential source was not called");
const sourceBackend = sourceClient.options.backend;
assert.ok(
  sourceBackend !== undefined && !(sourceBackend instanceof sdk.Backend),
);
assert.equal(sourceBackend.credentials, undefined);
await sourceClient.end();
// The native database key is a secret too.
const databaseKey = Uint8Array.from({ length: 32 }, (_, index) => index + 1);
const keyedClient = await sdk.Client.create(signer, {
  ...options,
  storage: {
    location: {
      directory: await mkdtemp(join(tmpdir(), "xmtp-sdk-conformance-key-")),
    },
    encryptionKey: databaseKey,
    singleConnection: false,
  },
});
assert.equal(keyedClient.options.storage.encryptionKey, undefined);
assertNoSecret(keyedClient.options, [databaseKey]);
await keyedClient.end();
console.log("Node scenario 3: credential update and 64-bit value passed");

const snapshot = reopened.serverConfiguration;
const fetched = await sdk.fetchServerConfiguration(backendOptions);
assert.equal(snapshot.identifier, fetched.identifier);
const staticBackend = await sdk.Backend.connect(backendOptions);
assert.equal(await sdk.Client.inboxIdFor(identity, staticBackend), inboxId);
assert.equal(
  (await sdk.Client.canMessage([identity], staticBackend)).get(
    `ethereum:${identity.identifier}`,
  ),
  true,
);
assert.equal(
  (await sdk.Client.canMessage([identity], backendOptions)).get(
    `ethereum:${identity.identifier}`,
  ),
  true,
);
const sameText = "1111111111111111111111111111111111111111";
const mixedIdentities: sdk.PublicIdentity[] = [
  { identifier: sameText, kind: "ethereum" },
  { identifier: sameText, kind: "passkey" },
  identity,
];
const checkMixedCanMessage = (result: Map<string, boolean>) => {
  assert.equal(result.size, 3);
  assert.equal(result.get(`ethereum:${sameText}`), false);
  assert.equal(result.get(`passkey:${sameText}`), false);
  assert.equal(result.get(`ethereum:${identity.identifier}`), true);
};
checkMixedCanMessage(await reopened.canMessage(mixedIdentities));
checkMixedCanMessage(
  await sdk.Client.canMessage(mixedIdentities, staticBackend),
);
checkMixedCanMessage(
  await sdk.Client.canMessage(mixedIdentities, backendOptions),
);
await assert.rejects(
  sdk.Client.build(
    identity,
    {
      ...options,
      backend: staticBackend,
      storage: {
        ...options.storage,
        location: "inMemory",
      },
    },
    inboxId,
  ),
  (error) => error instanceof sdk.XmtpError.IdentityNotFound,
);
assert.equal(
  (await reopened.refreshServerConfiguration()).identifier,
  snapshot.identifier,
);
await assert.rejects(
  sdk.fetchServerConfiguration({
    ...backendOptions,
    url: "http://127.0.0.1:1",
  }),
  (error) => error instanceof sdk.XmtpError.ConfigurationUnavailable,
);
console.log("Node scenario 10: configuration and typed error passed");

await logging(reopened, snapshot);
const local = await sdk.generateLocalSigner();
await assert.rejects(
  sdk.localSignerFromPrivateKey(new Uint8Array(31)),
  (error) => error instanceof sdk.XmtpError.InvalidInput,
);
const unsigned = await sdk.Client.create(local, {
  ...options,
  storage: { ...options.storage, location: "inMemory" },
  registration: { auto: false },
});
assert.equal(await unsigned.isRegistered(), false);
const request = await unsigned.unsafeCreateInboxSignatureRequest();
assert.ok(request);
assert.ok((await request.signatureText()).length > 0);
await request.sign(local);
await unsigned.unsafeApplySignatureRequest(request);
assert.equal(await unsigned.isRegistered(), true);
await unsigned.end();
console.log("Node scenario 11: local signer and signature request passed");
await metadataFields(options);
console.log("Node metadata fields and profiles passed");

// verifies: IDENT-073, IDENT-074, IDENT-075, IDENT-076
function recordingSigner(calls: string[]): sdk.Signer {
  const wallet = privateKeyToAccount(generatePrivateKey());
  return {
    async identity() {
      return { identifier: wallet.address.toLowerCase(), kind: "ethereum" };
    },
    async kind() {
      return { kind: "eoa" };
    },
    async sign(request) {
      calls.push("sign");
      const signature = await wallet.signMessage({ message: request.text });
      return { kind: "ecdsa", value: Uint8Array.from(toBytes(signature)) };
    },
  };
}
function preAuthenticateOptions(
  calls: string[],
  fail: boolean,
  auto: boolean,
): sdk.ClientOptions {
  return {
    ...options,
    storage: { ...options.storage, location: "inMemory" },
    registration: { auto },
    handlers: {
      preAuthenticate: {
        async run() {
          calls.push("pre-authenticate");
          if (fail) throw new Error("pre-authentication failed");
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
await preAuthenticated.register();
assert.deepEqual(preAuthCalls, ["pre-authenticate", "sign"]);
preAuthCalls.length = 0;
await preAuthenticated.register();
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

const familyGroup = await reopened.conversations.createGroup([], {
  name: "family group",
});
assert.equal((await familyGroup.state()).name, "family group");
assert.equal(familyGroup.creatorInboxId, inboxId);
assert.ok(
  (await reopened.conversations.listGroups(undefined)).some(
    (value) => value.id === familyGroup.id,
  ),
);
console.log("Node scenario 4: group options, state, and list passed");

const parentId = await familyGroup.sendText("parent");
const reactionId = await reopened.conversations.reactToMessage(parentId, {
  content: "👍",
  action: "added",
  schema: "unicode",
});
const replyId = await reopened.conversations.replyToMessage(
  parentId,
  sdk.encodeText("reply"),
);
assert.equal(
  (await reopened.decodeContent(sdk.encodeText("decoded"))).kind,
  "text",
);
const familyMessages = await familyGroup.messages();
const parent = familyMessages.find((value) => value.id === parentId);
const reply = familyMessages.find((value) => value.id === replyId);
assert.equal(parent?.reactions[0]?.id, reactionId);
assert.equal(parent?.replyCount, 1n);
assert.equal(reply?.inReplyTo?.id, parentId);
const reactionMessage = await reopened.conversations.getMessageById(reactionId);
if (reactionMessage?.content.kind !== "reaction")
  throw new Error("reaction message did not lift as a reaction");
assert.equal(reactionMessage.content.reference, parentId);
assert.equal(reactionMessage.content.referenceInboxId, inboxId);
assert.equal(reactionMessage.content.reaction.content, "👍");
console.log("Node scenario 5: message records, reaction, and reply passed");

const customType: sdk.ContentTypeId = {
  authorityId: "example.org",
  typeId: "sample",
  versionMajor: 1,
  versionMinor: 0,
};
const customCodec: sdk.ContentCodec<string> = {
  type: customType,
  encode(value) {
    return {
      type: customType,
      parameters: new Map(),
      content: new TextEncoder().encode(value),
    };
  },
  decode(value) {
    return new TextDecoder().decode(value.content);
  },
};
const ownerWithCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [customCodec] },
  inboxId,
);
const ownerWithoutCodec = await sdk.Client.build(identity, options, inboxId);
// A codec for "example.org" / "a/b" must not decode "example.org/a" / "b".
const slashType: sdk.ContentTypeId = {
  authorityId: "example.org",
  typeId: "a/b",
  versionMajor: 1,
  versionMinor: 0,
};
const slashCodec: sdk.ContentCodec<string> = {
  type: slashType,
  encode: (value) => ({ ...customCodec.encode(value), type: slashType }),
  decode: () => "wrong codec",
};
const slashHost = await sdk.Client.build(
  identity,
  { ...options, codecs: [slashCodec] },
  inboxId,
);
const colliding: sdk.EncodedContent = {
  type: {
    authorityId: "example.org/a",
    typeId: "b",
    versionMajor: 1,
    versionMinor: 0,
  },
  parameters: new Map(),
  content: new Uint8Array([1]),
};
const collidingId = await familyGroup.send(colliding);
const collidingMessage =
  await slashHost.conversations.getMessageById(collidingId);
assert.equal(
  collidingMessage?.content.kind,
  "unknown",
  "codec key collision selected the wrong codec",
);
await slashHost.end();
const customId = await familyGroup.send(customCodec.encode("codec value"));
const decoded = await ownerWithCodec.conversations.getMessageById(customId);
const undecoded =
  await ownerWithoutCodec.conversations.getMessageById(customId);
const customReplyId = await ownerWithCodec.conversations.replyToMessage(
  customId,
  customCodec.encode("reply codec value"),
);
const customReply =
  await ownerWithCodec.conversations.getMessageById(customReplyId);
const undecodedReply =
  await ownerWithoutCodec.conversations.getMessageById(customReplyId);
// Without a client codec, custom content is unknown and keeps the original
// serialized bytes from Rust, the same bytes the decoded custom item keeps.
if (undecoded?.content.kind !== "unknown")
  throw new Error("stored custom content was not unknown");
if (decoded?.content.kind !== "custom")
  throw new Error("custom message was not decoded");
assert.ok(
  undecoded.content.rawBytes.byteLength > undecoded.encoded!.content.byteLength,
);
assert.deepEqual(undecoded.content.rawBytes, decoded.content.rawBytes);
assert.equal(decoded.content.value, "codec value");
assert.equal(undecodedReply?.replyContent?.kind, "unknown");
if (customReply?.replyContent?.kind !== "custom")
  throw new Error("custom reply was not decoded");
assert.equal(customReply.replyContent.value, "reply codec value");
const failingCodec: sdk.ContentCodec<string> = {
  ...customCodec,
  decode() {
    throw new Error("codec decode failed");
  },
};
const ownerWithFailingCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [failingCodec] },
  inboxId,
);
const failedDecode =
  await ownerWithFailingCodec.conversations.getMessageById(customId);
if (failedDecode?.content.kind !== "custom")
  throw new Error("failed custom decode did not keep its content");
assert.match(failedDecode.content.error?.message ?? "", /codec decode failed/);
assert.equal(failedDecode.content.error?.code, "CodecDecodeFailed");
assert.equal(failedDecode.content.error?.category, "callback");
assert.equal(failedDecode.content.error?.retryable, false);
assert.deepEqual(failedDecode.content.rawBytes, decoded.content.rawBytes);
// verifies: CTYPE-008, CTYPE-009, CTYPE-029
const failedNestedReply =
  await ownerWithFailingCodec.conversations.getMessageById(customReplyId);
if (failedNestedReply?.content.kind !== "unknown")
  throw new Error("nested Node host failure did not retain the outer reply");
assert.equal(failedNestedReply.content.error.code, "CodecDecodeFailed");
assert.equal(failedNestedReply.content.error.category, "callback");
assert.equal(failedNestedReply.content.error.retryable, false);
assert.deepEqual(failedNestedReply.content.rawBytes, customReply.rawBytes);
assert.equal(failedNestedReply.content.encoded?.fallback, customReply.fallback);
assert.ok(customReply.fallback);
const normalReplyId = await ownerWithFailingCodec.conversations.replyToMessage(
  customId,
  sdk.encodeText("valid reply with failed parent"),
);
const replyWithFailedParent =
  await ownerWithFailingCodec.conversations.getMessageById(normalReplyId);
assert.equal(replyWithFailedParent?.content.kind, "reply");
assert.equal(replyWithFailedParent?.replyContent?.kind, "text");
if (replyWithFailedParent?.inReplyToContent?.kind !== "custom")
  throw new Error("Node parent custom failure missing");
assert.equal(
  replyWithFailedParent.inReplyToContent.error?.code,
  "CodecDecodeFailed",
);
assert.equal(
  replyWithFailedParent.inReplyToContent.error?.category,
  "callback",
);
assert.deepEqual(
  replyWithFailedParent.inReplyToContent.rawBytes,
  decoded.content.rawBytes,
);
console.log(
  "Node retained outer reply and isolated parent host failure passed",
);
await ownerWithFailingCodec.end();
const throwingCodec: sdk.ContentCodec<string> = {
  ...customCodec,
  decode(encoded) {
    const value = new TextDecoder().decode(encoded.content);
    if (value === "null prototype") throw Object.create(null);
    if (value === "throwing toString")
      throw {
        toString() {
          throw new Error("diagnostic failed");
        },
      };
    if (value === "bad decode") throw new Error("codec exploded");
    return value;
  },
};
const ownerWithThrowingCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [throwingCodec] },
  inboxId,
);
const throwingGroup = await ownerWithThrowingCodec.conversations.createGroup(
  [],
);
// verifies: PROC-045
const codecStream = sdk.MessageStream.openGroup(
  ownerWithThrowingCodec,
  throwingGroup,
);
const brokenId = await throwingGroup.send(customCodec.encode("bad decode"));
const broken = (await codecStream.next()).value;
assert.equal(broken?.id, brokenId);
if (broken?.content.kind !== "custom")
  throw new Error("a failed decode did not keep its custom content");
assert.match(broken.content.error?.message ?? "", /codec exploded/);
assert.equal(broken.content.error?.code, "CodecDecodeFailed");
assert.equal(broken.content.error?.category, "callback");
assert.ok(broken.content.rawBytes.byteLength > 0);
const continuedId = await throwingGroup.sendText("after codec error");
assert.equal((await codecStream.next()).value?.id, continuedId);
for (const hostile of ["null prototype", "throwing toString"]) {
  const failedId = await throwingGroup.send(customCodec.encode(hostile));
  const failed = (await codecStream.next()).value;
  assert.equal(failed?.id, failedId);
  if (failed?.content.kind !== "custom")
    throw new Error("hostile codec failure lost its custom content");
  assert.equal(failed.content.error?.code, "CodecDecodeFailed");
  assert.equal(failed.content.error?.category, "callback");
  assert.equal(failed.content.error?.retryable, false);
  assert.equal(failed.content.error?.message, "custom content codec failed");
  assert.equal(failed.content.value, undefined);
  assert.ok(failed.content.rawBytes.byteLength > 0);
  const goodId = await throwingGroup.send(
    customCodec.encode("after hostile failure"),
  );
  const good = (await codecStream.next()).value;
  assert.equal(good?.id, goodId);
  if (good?.content.kind !== "custom")
    throw new Error("valid item after hostile failure lost its custom content");
  assert.equal(good.content.value, "after hostile failure");
}
console.log(
  "Node stream delivered both hostile codec failures and the next valid items",
);
await codecStream.end();
await ownerWithThrowingCodec.end();
await ownerWithCodec.end();
await ownerWithoutCodec.end();
console.log("Node scenario 6: custom codec stayed with its client");

const archiveKey = new Uint8Array(32).fill(7);
const archive = await reopened.archives.exportToBytes(archiveKey, undefined);
assert.ok(archive instanceof Uint8Array && archive.byteLength > 0);
assert.equal(
  (await reopened.archives.metadataFromBytes(archive, archiveKey))
    .backupVersion,
  0,
);
const archiveDir = await mkdtemp(join(tmpdir(), "xmtp-sdk-archive-"));
try {
  const archivePath = join(archiveDir, "snapshot.xmtp");
  await reopened.archives.exportToFile(archivePath, archiveKey, undefined);
  assert.equal(
    (await reopened.archives.metadataFromFile(archivePath, archiveKey))
      .backupVersion,
    0,
  );
} finally {
  await rm(archiveDir, { recursive: true, force: true });
}
console.log("Node scenario 9: archive bytes and file passed");

// verifies: EVENT-014
// verifies: EVENT-050
// verifies: EVENT-053
const eventFilter: sdk.EventFilter = {
  kinds: ["conversation.joined"],
  references_own_messages: false,
};
const eventReader = await reopened.events(eventFilter);
let listenerCalls = 0;
const listenerId = await reopened.startListener(eventFilter, async () => {
  listenerCalls += 1;
});
await reopened.conversations.createGroup([]);
const sampleEvent = (await eventReader.next()).value;
assert.equal(sampleEvent?.kind, "conversation.joined");
for (let attempt = 0; attempt < 100 && listenerCalls === 0; attempt += 1)
  await new Promise((resolve) => setTimeout(resolve, 10));
assert.equal(listenerCalls, 1);
await reopened.stopListener(listenerId);
await eventReader.return();
console.log("Node scenario 8: event reader and listener passed");

// verifies: EVENT-014
// verifies: EVENT-053
const eventStream = await reopened.events(eventFilter);
assert.ok(eventStream instanceof sdk.EventStream);
await reopened.conversations.createGroup([]);
let publicEvents = 0;
for await (const event of eventStream) {
  assert.ok(event);
  publicEvents += 1;
  break;
}
assert.equal(publicEvents, 1, "public EventStream missed the event");
assert.deepEqual(await eventStream.next(), { done: true, value: undefined });
let endedReaders = 0;
const returnProbe = new HostEventStream({
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
await reopened.conversations.createGroup([]);
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
await reopened.conversations.createGroup([]);
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
  await reopened.conversations.createGroup([]);
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
