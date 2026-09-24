import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { mkdtemp, readdir } from "node:fs/promises";
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
await sdk.uniffiInitAsync();
assert.match(sdk.sdkVersion(), /^1\.12\.0/);
console.log("Node scenario 1: load, checksums, version passed");

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
const group = await client.conversations().createGroup([]);
const sentID = await group.sendText("conformance message");
const history = await group.messages();
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
  const shortGroup = await shortLived.conversations().createGroup([]);
  const id = await shortGroup.sendText("weak owner");
  releasedMessage = (await shortGroup.messages()).find(
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

const reopenedGroup = await reopened.conversations().createGroup([]);
const reader = await reopenedGroup.messageReader();
const messageID = await reopenedGroup.sendText("durable stream");
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
const protocolGroup = await reopened.conversations().createGroup([]);
const firstID = await protocolGroup.sendText("ack on request");
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
const secondID = await protocolGroup.sendText("second request");
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
await opening.return();
resolveCreation({
  next: async () => undefined,
  end: async () => {
    endedLate = true;
  },
});
assert.equal((await openingRead).done, true);
await new Promise((resolve) => setTimeout(resolve, 0));
assert.equal(endedLate, true, "late reader remained open");
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

sdk.initLogging({
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

await reopened.end();
console.log("Node scenario 7: durable stream and idle cancellation passed");
