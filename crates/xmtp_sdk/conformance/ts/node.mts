import assert from "node:assert/strict";
import { realpathSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
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
    url: process.env.XMTP_BACKEND_URL ?? "http://127.0.0.1:9150",
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
assert.throws(() => sent.client(), /clientClosed/);

const reopened = await sdk.Client.build(identity, options, inboxID);
assert.equal(reopened.inboxID().toString(), inboxID.toString());
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
await reopened.end();
console.log("Node scenario 7: durable stream and idle cancellation passed");
