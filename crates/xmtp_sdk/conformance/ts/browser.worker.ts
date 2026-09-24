import {
  generatePrivateKey,
  privateKeyToAccount,
} from "../../../../sdks/browser/node_modules/viem/_esm/accounts/index.js";
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/index.js";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index.ts";

async function run(): Promise<void> {
  console.log("loading SDK WASM");
  await sdk.uniffiInitAsync(
    new URL(
      "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.wasm",
      import.meta.url,
    ),
  );
  console.log("SDK WASM loaded");
  if (!sdk.sdkVersion().startsWith("1.12.0")) throw new Error("wrong version");
  sdk.MessageID.fromString("a".repeat(64));
  postMessage({ result: "Browser scenario 1 passed" });

  const account = privateKeyToAccount(generatePrivateKey());
  const identity = {
    identifier: account.address.toLowerCase(),
    kind: sdk.PublicIdentityKind.Ethereum,
  };
  const signer = {
    async identity() {
      console.log("signer.identity");
      return identity;
    },
    async kind() {
      console.log("signer.kind");
      return new sdk.SignerKind.Eoa();
    },
    async sign(request: { text: string }) {
      console.log("signer.sign");
      const bytes = toBytes(
        await account.signMessage({ message: request.text }),
      );
      return new sdk.Signature.Ecdsa(Uint8Array.from(bytes).buffer);
    },
  };
  const options = {
    backend: {
      url: "http://127.0.0.1:9150",
      appVersion: undefined,
      credentials: undefined,
    },
    storage: {
      location: new sdk.StorageLocation.Directory("xmtp-sdk-conformance"),
      label: crypto.randomUUID(),
      encryptionKey: undefined,
    },
    deviceSync: false,
  };
  if (!("storage" in navigator) || !navigator.storage.getDirectory) {
    throw new Error("OPFS is unavailable in the dedicated worker");
  }
  console.log("OPFS available; creating client");
  const client = await Promise.race([
    sdk.Client.create(signer, options),
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("Client.create timed out")), 30000),
    ),
  ]);
  console.log("browser client created");
  const inboxID = client.inboxID();
  const group = await client.conversations().createGroup([]);
  const sentID = await group.sendText("browser conformance");
  const sent = (await group.messages()).find(
    (message) => message.id.toString() === sentID.toString(),
  );
  if (!(sent instanceof sdk.Message) || sent.client() !== client)
    throw new Error("message lift failed");
  await client.end();
  try {
    sent.client();
    throw new Error("ended client remained in registry");
  } catch (error) {
    if (!String(error).includes("clientClosed")) throw error;
  }
  const reopened = await sdk.Client.build(identity, options, inboxID);
  if (reopened.inboxID().toString() !== inboxID.toString())
    throw new Error("inbox changed");
  postMessage({ result: "Browser scenario 2 passed" });

  const liveGroup = await reopened.conversations().createGroup([]);
  const reader = await liveGroup.messageReader();
  const liveID = await liveGroup.sendText("durable stream");
  if ((await reader.next())?.id.toString() !== liveID.toString())
    throw new Error("first delivery missing");
  await reader.end();
  const replay = await liveGroup.messageReader();
  if ((await replay.next())?.id.toString() !== liveID.toString())
    throw new Error("replay missing");
  await replay.end();
  const stream = new sdk.MessageStream(
    (signal) => liveGroup.messageReader({ signal }),
    reopened,
  );
  if ((await stream.next()).value?.id.toString() !== liveID.toString())
    throw new Error("stream did not redeliver");
  const pending = stream.next();
  setTimeout(() => void stream.return(), 50);
  if (!(await pending).done) throw new Error("idle read was not cancelled");
  await stream.return();
  await reopened.end();
  postMessage({ result: "Browser scenario 7 passed" });
}

run().then(
  () => postMessage({ result: "PASS" }),
  (error) =>
    postMessage({ result: `FAIL: ${String(error)}\n${error?.stack ?? ""}` }),
);
