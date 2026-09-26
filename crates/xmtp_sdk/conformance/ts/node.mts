import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

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
  () => sdk.MessageID.fromString("bad"),
  sdk.XmtpError.InvalidArgument,
);
await sdk.uniffiInitAsync();
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
console.log("Node scenario 1: load, checksums, version passed");

const standardCodecs = new Map([
  [sdk.StandardContent_Tags.Text, new sdk.TextCodec()],
  [sdk.StandardContent_Tags.Markdown, new sdk.MarkdownCodec()],
  [sdk.StandardContent_Tags.ReadReceipt, new sdk.ReadReceiptCodec()],
  [sdk.StandardContent_Tags.Reaction, new sdk.ReactionV2Codec()],
  [sdk.StandardContent_Tags.Attachment, new sdk.AttachmentCodec()],
  [sdk.StandardContent_Tags.RemoteAttachment, new sdk.RemoteAttachmentCodec()],
  [sdk.StandardContent_Tags.MultiRemoteAttachment, new sdk.MultiRemoteAttachmentCodec()],
  [sdk.StandardContent_Tags.TransactionReference, new sdk.TransactionReferenceCodec()],
  [sdk.StandardContent_Tags.WalletSendCalls, new sdk.WalletSendCallsCodec()],
  [sdk.StandardContent_Tags.Actions, new sdk.ActionsCodec()],
  [sdk.StandardContent_Tags.Intent, new sdk.IntentCodec()],
  [sdk.StandardContent_Tags.Reply, new sdk.ReplyCodec()],
  [sdk.StandardContent_Tags.GroupUpdated, new sdk.GroupUpdatedCodec()],
  [sdk.StandardContent_Tags.DeleteMessage, new sdk.DeleteMessageCodec()],
  [sdk.StandardContent_Tags.LeaveRequest, new sdk.LeaveRequestCodec()],
]);
const codecSamples = sdk.sdkConformanceStandardSamples();
assert.equal(codecSamples.length, 15);
function assertEncodedEqual(actual: sdk.EncodedContent, expected: sdk.EncodedContent): void {
  assert.deepEqual(actual.type, expected.type);
  assert.deepEqual(actual.parameters, expected.parameters);
  assert.equal(actual.fallback, expected.fallback);
  assert.deepEqual(Buffer.from(actual.content), Buffer.from(expected.content));
}
for (const sample of codecSamples) {
  const codec = standardCodecs.get(sample.value.tag);
  assert.ok(codec, `missing codec for ${sample.value.tag}`);
  const value =
    sample.value.tag === sdk.StandardContent_Tags.ReadReceipt
      ? undefined
      : sample.value.tag === sdk.StandardContent_Tags.Reaction ||
          sample.value.tag === sdk.StandardContent_Tags.Reply ||
          sample.value.tag === sdk.StandardContent_Tags.DeleteMessage
        ? sample.value
        : sample.value.inner[0];
  const encoded = codec.encode(value);
  assertEncodedEqual(encoded, sample.expected);
  assertEncodedEqual(codec.encode(codec.decode(encoded)), sample.expected);
}
console.log("Node P69: all 15 standard codecs match Rust bytes");

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
const inboxID = client.inboxID();
assert.equal(typeof inboxID.toString(), "string");
const storagePath = await client.storage().path();
assert.ok(storagePath);
assert.ok((await stat(storagePath)).isFile());
const group = await client.conversations().createGroup([], undefined);
let typedSends = 0;
for (const sample of codecSamples) {
  const value = sample.value;
  let id: sdk.MessageID;
  switch (value.tag) {
    case sdk.StandardContent_Tags.Text: id = await group.sendText(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.Markdown: id = await group.sendMarkdown(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.Reaction: id = await group.sendReaction(value.inner.reference, value.inner.referenceInboxID, value.inner.reaction, undefined); break;
    case sdk.StandardContent_Tags.Reply: id = await group.sendReply(value.inner.reference, value.inner.referenceInboxID, value.inner.content, undefined); break;
    case sdk.StandardContent_Tags.ReadReceipt: id = await group.sendReadReceipt(undefined); break;
    case sdk.StandardContent_Tags.Attachment: id = await group.sendAttachment(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.RemoteAttachment: id = await group.sendRemoteAttachment(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.MultiRemoteAttachment: id = await group.sendMultiRemoteAttachment(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.TransactionReference: id = await group.sendTransactionReference(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.WalletSendCalls: id = await group.sendWalletSendCalls(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.Actions: id = await group.sendActions(value.inner[0], undefined); break;
    case sdk.StandardContent_Tags.Intent: id = await group.sendIntent(value.inner[0], undefined); break;
    default: continue;
  }
  const wire = await client.conversations().getMessageByID(id);
  assert.ok(wire);
  assertEncodedEqual(wire.encoded, sample.expected);
  typedSends++;
}
assert.equal(typedSends, 12);
console.log("Node P69: typed send bytes match all 12 public codecs");
const sentID = await group.sendText("conformance message", undefined);
const history = await group.messages(undefined);
const sent = history.find(
  (message) => message.id.toString() === sentID.toString(),
);
assert.ok(sent instanceof sdk.Message);
assert.equal(sent.client(), client);
await client.end();
assert.throws(
  () => sent.client(),
  (error) => error instanceof sdk.XmtpError.ClientClosed,
);

const reopened = await sdk.Client.build(identity, options, inboxID);
assert.equal(reopened.inboxID().toString(), inboxID.toString());
const defaultRoot = await mkdtemp(join(tmpdir(), "xmtp-sdk-default-"));
const oldCwd = process.cwd();
process.chdir(defaultRoot);
try {
  const defaultClient = await sdk.Client.build(
    identity,
    {
      ...options,
      storage: {
        ...options.storage,
        location: new sdk.StorageLocation.Default(),
      },
    },
    inboxID,
  );
  assert.ok(
    (await readdir(join(defaultRoot, "xmtp"))).some((name) =>
      name.endsWith(".db3"),
    ),
  );
  await defaultClient.end();
} finally {
  process.chdir(oldCwd);
}
let releasedMessage: sdk.Message;
const weak = await (async () => {
  const shortLived = await sdk.Client.build(identity, options, inboxID);
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

const reopenedGroup = await reopened.conversations().createGroup([], undefined);
const reader = await reopenedGroup.messageReader();
const messageID = await reopenedGroup.sendText("durable stream", undefined);
const first = await reader.next();
assert.equal(first?.id.toString(), messageID.toString());
await reader.end();
const replay = await reopenedGroup.messageReader();
const repeated = await replay.next();
assert.equal(repeated?.id.toString(), messageID.toString());
await replay.end();
const stream = new sdk.MessageStream(
  (signal) => reopenedGroup.messageReader({ signal }),
  reopened,
);
assert.equal((await stream.next()).value?.id.toString(), messageID.toString());
const pending = stream.next();
setTimeout(() => void stream.return(), 50);
assert.equal((await pending).done, true);
await stream.return();
const protocolGroup = await reopened.conversations().createGroup([], undefined);
const firstID = await protocolGroup.sendText("ack on request", undefined);
const firstStream = new sdk.MessageStream(
  (signal) => protocolGroup.messageReader({ signal }),
  reopened,
);
assert.equal(
  (await firstStream.next()).value?.id.toString(),
  firstID.toString(),
);
await firstStream.return();
const secondStream = new sdk.MessageStream(
  (signal) => protocolGroup.messageReader({ signal }),
  reopened,
);
let replayTimer: ReturnType<typeof setTimeout>;
const replayedItem = await Promise.race([
  secondStream.next(),
  new Promise<never>((_, reject) => {
    replayTimer = setTimeout(
      () => reject(new Error("adapter prefetched and acknowledged the item")),
      3_000,
    );
  }),
]).finally(() => clearTimeout(replayTimer));
assert.equal(
  replayedItem.value?.id.toString(),
  firstID.toString(),
  "item was prefetched and acknowledged",
);
const secondID = await protocolGroup.sendText("second request", undefined);
assert.equal(
  (await secondStream.next()).value?.id.toString(),
  secondID.toString(),
);
await secondStream.return();
const afterAck = await protocolGroup.messageReader();
assert.equal(
  (await afterAck.next())?.id.toString(),
  secondID.toString(),
  "first item was not acknowledged on next request",
);
await afterAck.end();
const breakGroup = await reopened.conversations().createGroup([], undefined);
const breakID = await breakGroup.sendText("close after break");
const breakReasons: sdk.StreamCloseReason[] = [];
const retainedStream = new sdk.MessageStream(
  (signal) => breakGroup.messageReader({ signal }),
  reopened,
  { onClose: (reason) => breakReasons.push(reason) },
);
for await (const value of retainedStream) {
  assert.equal(value.id.toString(), breakID.toString());
  break;
}
assert.deepEqual(
  breakReasons.map((reason) => reason.kind),
  ["closed"],
  "break did not close the stored stream",
);
const breakReplay = await breakGroup.messageReader();
assert.equal(
  (await breakReplay.next())?.id.toString(),
  breakID.toString(),
  "break acknowledged the last message",
);
await breakReplay.end();

let resolveCreation!: (reader: {
  next: () => Promise<undefined>;
  end: () => Promise<void>;
}) => void;
let endedLate = false;
const opening = new sdk.MessageStream(
  () =>
    new Promise((resolve) => {
      resolveCreation = resolve;
    }),
  reopened,
);
const openingRead = opening.next();
await opening.end();
assert.equal((await openingRead).done, true, "opening read did not settle");
resolveCreation({
  next: async () => undefined,
  end: async () => {
    endedLate = true;
  },
});
await new Promise((resolve) => setTimeout(resolve, 0));
assert.equal(endedLate, true, "late reader remained open");
const closeReasons: sdk.StreamCloseReason[] = [];
const explicitlyClosed = new sdk.MessageStream(
  async () => ({ next: async () => undefined, end: async () => {} }),
  reopened,
  { onClose: (reason) => closeReasons.push(reason) },
);
await explicitlyClosed.end();
assert.deepEqual(
  closeReasons.map((reason) => reason.kind),
  ["closed"],
);
let endedAfterCloseThrow = false;
const throwingClose = new sdk.MessageStream(
  async () => ({
    next: async () => undefined,
    end: async () => {
      endedAfterCloseThrow = true;
    },
  }),
  reopened,
  {
    onClose: () => {
      throw new Error("close callback failed");
    },
  },
);
await throwingClose.ready();
await assert.rejects(throwingClose.end(), /close callback failed/);
assert.equal(endedAfterCloseThrow, true, "throwing onClose skipped reader.end");
let endedAfterFailureCloseThrow = false;
const throwingFailureClose = new sdk.MessageStream(
  async () => ({
    next: async () => {
      throw new Error("reader failed");
    },
    end: async () => {
      endedAfterFailureCloseThrow = true;
    },
  }),
  reopened,
  {
    onClose: () => {
      throw new Error("failure callback failed");
    },
  },
);
await throwingFailureClose.ready();
await assert.rejects(throwingFailureClose.next(), /failure callback failed/);
assert.equal(
  endedAfterFailureCloseThrow,
  true,
  "throwing failure callback skipped reader.end",
);
let stateCallbackCalls = 0;
const throwingState = new sdk.MessageStream(
  async () => ({
    next: async () => undefined,
    end: async () => {},
    connectionState: () => sdk.ConnectionState.Connected,
    connectionStateChanged: async () => sdk.ConnectionState.Closed,
  }),
  reopened,
  {
    onConnectionStateChange: () => {
      stateCallbackCalls += 1;
      throw new Error("state callback failed");
    },
  },
);
await throwingState.ready();
await new Promise((resolve) => setTimeout(resolve, 10));
assert.equal(stateCallbackCalls, 1);
await throwingState.end();
for (const code of [
  "recoveryExhausted",
  "storage",
  "lagged",
  "credentialRejected",
  "credentialExhausted",
  "backendMismatch",
  "clientVersionTooOld",
  "consumerOwned",
  "foreignCursor",
]) {
  const failure = Object.assign(new Error(code), { code });
  const reasons: sdk.StreamCloseReason[] = [];
  const failing = new sdk.MessageStream(
    async () => ({
      next: async () => {
        throw failure;
      },
      end: async () => {},
    }),
    reopened,
    { onClose: (reason) => reasons.push(reason) },
  );
  await assert.rejects(failing.next(), (error) => error === failure);
  assert.equal(reasons.length, 1);
  assert.equal(reasons[0].kind, "failed");
  if (reasons[0].kind === "failed")
    assert.equal((reasons[0].error as { code: string }).code, code);
}
for (const StreamType of [sdk.MessageStream, sdk.ConversationStream]) {
  const states: sdk.ConnectionState[] = [];
  const changes: Array<(state: sdk.ConnectionState) => void> = [];
  const probe = new StreamType(
    async () => ({
      next: async () => undefined,
      end: async () => {},
      connectionState: () => sdk.ConnectionState.Connected,
      connectionStateChanged: () =>
        new Promise<sdk.ConnectionState>((resolve) => changes.push(resolve)),
    }),
    reopened,
    { onConnectionStateChange: (_previous, current) => states.push(current) },
  );
  await probe.ready();
  assert.deepEqual(states, [
    sdk.ConnectionState.Connecting,
    sdk.ConnectionState.Connected,
  ]);
  changes.shift()?.(sdk.ConnectionState.Reconnecting);
  await new Promise((resolve) => setTimeout(resolve, 0));
  changes.shift()?.(sdk.ConnectionState.Connected);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(states, [
    sdk.ConnectionState.Connecting,
    sdk.ConnectionState.Connected,
    sdk.ConnectionState.Reconnecting,
    sdk.ConnectionState.Connected,
  ]);
  await probe.end();
}
let closedStatePolls = 0;
const closedStateProbe = new sdk.MessageStream(
  async () => ({
    next: async () => undefined,
    end: async () => {},
    connectionState: () => sdk.ConnectionState.Closed,
    connectionStateChanged: async () => {
      closedStatePolls += 1;
      if (closedStatePolls > 2) throw new Error("closed state loop");
      return sdk.ConnectionState.Closed;
    },
  }),
  reopened,
  { onConnectionStateChange: () => {} },
);
await closedStateProbe.ready();
await new Promise((resolve) => setTimeout(resolve, 0));
assert.equal(closedStatePolls, 0, "closed state kept the monitor running");
await closedStateProbe.end();
const callbackGroup = await reopened.conversations().createGroup([], undefined);
const callbackID = await callbackGroup.sendText("callback acknowledgment");
let releaseCallback!: () => void;
const callbackGate = new Promise<void>((resolve) => {
  releaseCallback = resolve;
});
let callbackEntered!: () => void;
const entered = new Promise<void>((resolve) => {
  callbackEntered = resolve;
});
const callbackStream = new sdk.MessageStream(
  (signal) => callbackGroup.messageReader({ signal }),
  reopened,
);
const consumption = callbackStream.onValue(async (value) => {
  assert.equal(value.id.toString(), callbackID.toString());
  callbackEntered();
  await callbackGate;
});
await entered;
await callbackStream.end();
const callbackReplay = await callbackGroup.messageReader();
assert.equal(
  (await callbackReplay.next())?.id.toString(),
  callbackID.toString(),
);
await callbackReplay.end();
releaseCallback();
await consumption;
const conversationStream = new sdk.ConversationStream(
  (signal) =>
    reopened.conversations().conversationReader(undefined, { signal }),
  reopened,
);
await conversationStream.ready();
await reopened.conversations().createGroup([], undefined);
assert.equal((await conversationStream.next()).done, false);
await conversationStream.end();
const rejectedOpening = new sdk.MessageStream(
  (signal) =>
    new Promise((_, reject) => {
      signal.addEventListener("abort", () =>
        reject(new DOMException("aborted", "AbortError")),
      );
    }),
  reopened,
);
const rejectedRead = rejectedOpening.next();
await rejectedOpening.return();
assert.equal((await rejectedRead).done, true);
const creationFailure = Object.assign(new Error("reader creation failed"), {
  code: "storage",
});
const creationReasons: sdk.StreamCloseReason[] = [];
const failedOpening = new sdk.MessageStream(
  async () => {
    throw creationFailure;
  },
  reopened,
  { onClose: (reason) => creationReasons.push(reason) },
);
await assert.rejects(
  failedOpening.next(),
  (error) => error === creationFailure,
);
assert.equal(creationReasons[0]?.kind, "failed");

let readerLeaseHeld = false;
const readFailure = new Error("injected reader failure");
const failedStream = new sdk.MessageStream(async () => {
  assert.equal(readerLeaseHeld, false);
  readerLeaseHeld = true;
  return {
    next: async () => {
      throw readFailure;
    },
    end: async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
      readerLeaseHeld = false;
    },
  };
}, reopened);
await assert.rejects(
  async () => {
    for await (const message of failedStream) {
      assert.fail(`unexpected message: ${message.id}`);
    }
  },
  (error) => error === readFailure,
);
const replacementStream = new sdk.MessageStream(async () => {
  assert.equal(readerLeaseHeld, false, "failed stream kept the reader lease");
  readerLeaseHeld = true;
  return {
    next: async () => first!,
    end: async () => {
      readerLeaseHeld = false;
    },
  };
}, reopened);
assert.equal(
  (await replacementStream.next()).value?.id.toString(),
  messageID.toString(),
);
await replacementStream.return();
const endFailure = new Error("injected reader end failure");
const failedEndStream = new sdk.MessageStream(
  async () => ({
    next: async () => {
      throw readFailure;
    },
    end: async () => {
      throw endFailure;
    },
  }),
  reopened,
);
await assert.rejects(failedEndStream.next(), (error) => error === readFailure);

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
  storage: { ...options.storage, location: new sdk.StorageLocation.InMemory() },
};
const credentialClient = await sdk.Client.build(
  identity,
  credentialOptions,
  inboxID,
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
  inboxID,
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
    await sdk.Client.inboxIDFor(
      identity,
      new sdk.BackendSource.Connected({ backend: staticBackend }),
    )
  ).toString(),
  inboxID.toString(),
);
assert.equal(
  (
    await sdk.Client.canMessage(
      [identity],
      new sdk.BackendSource.Connected({ backend: staticBackend }),
    )
  )[0]?.canMessage,
  true,
);
assert.equal(
  (
    await sdk.Client.canMessage(
      [identity],
      new sdk.BackendSource.Options({ options: backendOptions }),
    )
  )[0]?.canMessage,
  true,
);
const connectedClient = await sdk.Client.build(
  identity,
  {
    ...options,
    backend: new sdk.BackendSource.Connected({ backend: staticBackend }),
    storage: {
      ...options.storage,
      location: new sdk.StorageLocation.InMemory(),
    },
  },
  inboxID,
);
await connectedClient.end();
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

await sdk.initLogging({
  level: sdk.LogLevel.Error,
  structured: true,
  performance: false,
  otel: undefined,
  resourceAttributes: new Map(),
});
let sinkDelivered!: () => void;
const sinkRecord = new Promise<void>((resolve) => {
  sinkDelivered = resolve;
});
let sinkError: unknown;
sdk.setLogSink({
  log(record) {
    try {
      assert.ok(record.target.length > 0);
      assert.ok(record.level !== undefined);
      assert.ok(record.fields instanceof Map);
      assert.equal(typeof record.droppedRecords, "bigint");
      assert.equal(
        reopened.raw.serverConfiguration().identifier,
        snapshot.identifier,
      );
    } catch (error) {
      sinkError = error;
    }
    sinkDelivered();
  },
});
await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31).buffer));
await Promise.race([
  sinkRecord,
  new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error("queued log sink did not run")), 3_000),
  ),
]);
sdk.setLogSink(undefined);
if (sinkError !== undefined) throw sinkError;
console.log("Node logging: queued sink called Rust without a deadlock");
let sinkThrew = false;
sdk.setLogSink({
  log() {
    sinkThrew = true;
    throw new Error("test sink failure");
  },
});
await assert.rejects(sdk.localSignerFromPrivateKey(new Uint8Array(31).buffer));
for (let attempt = 0; attempt < 30 && !sinkThrew; attempt += 1) {
  await new Promise((resolve) => setTimeout(resolve, 10));
}
sdk.setLogSink(undefined);
assert.equal(sinkThrew, true, "failing sink was not called");
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
console.log("Node logging: sink error did not stop the process");

const loggingChild = fileURLToPath(
  new URL("./logging-child.mts", import.meta.url),
);
await new Promise<void>((resolve, reject) => {
  const child = spawn(
    process.execPath,
    [
      "--import",
      realpathSync(
        fileURLToPath(
          new URL(
            "../../../../sdks/node/node_modules/tsx/dist/loader.mjs",
            import.meta.url,
          ),
        ),
      ),
      loggingChild,
    ],
    { env: process.env, stdio: "inherit" },
  );
  const timeout = setTimeout(() => {
    child.kill("SIGKILL");
    reject(new Error("inline log sink deadlocked while Rust held a lock"));
  }, 5_000);
  child.on("error", (error) => {
    clearTimeout(timeout);
    reject(error);
  });
  child.on("exit", (code) => {
    clearTimeout(timeout);
    if (code === 0) resolve();
    else reject(new Error(`logging child exited with ${code}`));
  });
});
console.log("Node logging: queued sink avoided the lock inversion");

let droppedRecords = 0n;
let firstRecord = true;
sdk.setLogSink({
  log(record) {
    if (firstRecord) {
      firstRecord = false;
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
    }
    if (record.droppedRecords > droppedRecords)
      droppedRecords = record.droppedRecords;
  },
});
await sdk.sdkConformanceEmit(10_000);
for (let attempt = 0; attempt < 100 && droppedRecords === 0n; attempt += 1) {
  await new Promise((resolve) => setTimeout(resolve, 50));
}
sdk.setLogSink(undefined);
assert.ok(
  droppedRecords > 0n,
  "the bounded log queue did not report dropped records",
);
console.log(
  `Node logging: queue overflow reported ${droppedRecords} dropped records`,
);

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

const familyGroup = await reopened.conversations().createGroup([], {
  permissions: undefined,
  name: "family group",
  imageUrl: undefined,
  description: undefined,
  disappearing: undefined,
  appData: undefined,
});
assert.equal((await familyGroup.state()).name, "family group");
assert.equal(familyGroup.creatorInboxID().toString(), inboxID.toString());
assert.ok(
  (await reopened.conversations().listGroups(undefined)).some(
    (value) => value.id().toString() === familyGroup.id().toString(),
  ),
);
console.log("Node scenario 4: group options, state, and list passed");

const parentID = await familyGroup.sendText("parent", undefined);
const reactionID = await reopened.conversations().reactToMessage(
  parentID,
  {
    content: "👍",
    action: sdk.ReactionAction.Added,
    schema: sdk.ReactionSchema.Unicode,
  },
  undefined,
);
const replyID = await reopened
  .conversations()
  .replyToMessage(parentID, sdk.encodeText("reply"), undefined);
assert.equal(
  (await reopened.raw.decodeContent(sdk.encodeText("decoded"))).tag,
  sdk.MessageContent_Tags.Text,
);
const familyMessages = await familyGroup.messages(undefined);
const parent = familyMessages.find(
  (value) => value.id.toString() === parentID.toString(),
);
const reply = familyMessages.find(
  (value) => value.id.toString() === replyID.toString(),
);
assert.equal(parent?.reactions[0]?.id.toString(), reactionID.toString());
assert.equal(parent?.replyCount, 1n);
assert.equal(reply?.inReplyTo?.id.toString(), parentID.toString());
console.log("Node scenario 5: message records, reaction, and reply passed");

const customType = sdk.ContentTypeID.create({
  authorityID: "example.org",
  typeID: "sample",
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
  inboxID,
);
const ownerWithoutCodec = await sdk.Client.build(identity, options, inboxID);
const customID = await familyGroup.send(
  customCodec.encode("codec value"),
  undefined,
);
const decoded = await ownerWithCodec.conversations().getMessageByID(customID);
const undecoded = await ownerWithoutCodec
  .conversations()
  .getMessageByID(customID);
const customReplyID = await ownerWithCodec
  .conversations()
  .replyToMessage(customID, customCodec.encode("reply codec value"), undefined);
const customReply = await ownerWithCodec
  .conversations()
  .getMessageByID(customReplyID);
const undecodedReply = await ownerWithoutCodec
  .conversations()
  .getMessageByID(customReplyID);
assert.equal(undecoded?.content.tag, sdk.MessageContent_Tags.Unknown);
const serializedCustom = new Uint8Array([10, 3, 1, 2, 3]).buffer;
const syntheticUnknown = new sdk.Message({
  clientKey: ownerWithoutCodec.raw.clientKey(),
  content: {
    tag: sdk.MessageContent_Tags.Custom,
    inner: { encoded: customCodec.encode("codec value"), rawBytes: serializedCustom },
  },
  inReplyTo: undefined,
} as sdk.MessageData);
assert.deepEqual(
  new Uint8Array((syntheticUnknown.content as { inner: { rawBytes: ArrayBuffer } }).inner.rawBytes),
  new Uint8Array(serializedCustom),
);
const rustRawBytes = (
  undecoded?.data.content as { inner: { rawBytes: ArrayBuffer } }
).inner.rawBytes;
assert.ok(new Uint8Array(rustRawBytes).byteLength > new Uint8Array(undecoded!.encoded.content).byteLength);
assert.deepEqual(
  new Uint8Array((undecoded?.content as { inner: { rawBytes: ArrayBuffer } }).inner.rawBytes),
  new Uint8Array(rustRawBytes),
);
assert.equal(undecodedReply?.replyContent?.tag, sdk.MessageBody_Tags.Unknown);
assert.equal(
  (customReply?.replyContent as { inner?: { value?: string } })?.inner?.value,
  "reply codec value",
);
assert.equal(
  (decoded?.content as { inner?: { value?: string } }).inner?.value,
  "codec value",
);
assert.equal(
  (undecoded?.content as { inner?: { value?: string } }).inner?.value,
  undefined,
);
const failingCodec = {
  ...customCodec,
  decode(_value: sdk.EncodedContent): string {
    throw new Error("codec decode failed");
  },
};
const ownerWithFailingCodec = await sdk.Client.build(
  identity,
  { ...options, codecs: [failingCodec] },
  inboxID,
);
const failedDecode = await ownerWithFailingCodec
  .conversations()
  .getMessageByID(customID);
assert.match(
  String(
    (failedDecode?.content as { inner?: { error?: string } }).inner?.error,
  ),
  /codec decode failed/,
);
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
  inboxID,
);
const throwingGroup = await ownerWithThrowingCodec
  .conversations()
  .createGroup([], undefined);
const codecStream = new sdk.MessageStream(
  (signal) => throwingGroup.messageReader({ signal }),
  ownerWithThrowingCodec,
);
const brokenID = await throwingGroup.send(
  customCodec.encode("bad decode"),
  undefined,
);
const broken = (await codecStream.next()).value;
assert.equal(broken?.id.toString(), brokenID.toString());
assert.equal(broken?.content.tag, sdk.MessageContent_Tags.Custom);
assert.match(
  (broken?.content as { inner?: { error?: string } }).inner?.error ?? "",
  /codec exploded/,
);
const continuedID = await throwingGroup.sendText("after codec error");
assert.equal(
  (await codecStream.next()).value?.id.toString(),
  continuedID.toString(),
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

await reopened.end();
console.log("Node scenario 7: durable stream and idle cancellation passed");
