import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import * as sdk from "../../../../target/sdk-generated/typescript-napi/index.ts";

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
const options = {
  backend: {
    url: process.env.XMTP_BACKEND_URL!,
    appVersion: undefined,
    credentials: undefined,
  },
  storage: {
    location: new sdk.StorageLocation.Directory(
      await mkdtemp(join(tmpdir(), "xmtp-sdk-conformance-")),
    ),
    label: undefined,
    encryptionKey: undefined,
  },
  deviceSync: false,
};

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
await reopened.end();
console.log("Node scenario 7: durable stream and idle cancellation passed");
