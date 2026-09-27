import {
  generatePrivateKey,
  privateKeyToAccount,
} from "../../../../sdks/browser/node_modules/viem/_esm/accounts/index.js";
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/index.js";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index.ts";

async function poolFilenames(): Promise<string[]> {
  const root = await navigator.storage.getDirectory();
  const metadata = await root.getDirectoryHandle(".opfs-libxmtp-metadata");
  const pool = await metadata.getDirectoryHandle(".opaque");
  const names: string[] = [];
  for await (const handle of pool.values()) {
    if (handle.kind !== "file") continue;
    // The SAH pool keeps the logical name in the first 512 bytes of each file.
    const bytes = new Uint8Array(
      await (await handle.getFile()).slice(0, 512).arrayBuffer(),
    );
    const end = bytes.indexOf(0);
    if (end > 0) names.push(new TextDecoder().decode(bytes.subarray(0, end)));
  }
  return names.sort();
}

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
    backend: new sdk.BackendSource.Options({
      options: {
        url: import.meta.env.VITE_XMTP_BACKEND_URL,
        appVersion: undefined,
        credentials: undefined,
        credential: undefined,
      },
    }),
    storage: {
      location: new sdk.StorageLocation.Directory("xmtp-sdk-conformance"),
      label: crypto.randomUUID(),
      encryptionKey: undefined,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    registration: { auto: true, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
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
  const group = await client.conversations().createGroup([], undefined);
  const sentID = await group.sendText("browser conformance", undefined);
  const sent = (await group.messages(undefined)).find(
    (message) => message.id.toString() === sentID.toString(),
  );
  if (!(sent instanceof sdk.Message) || sent.client() !== client)
    throw new Error("message lift failed");
  await client.end();
  const newInbox = sdk.InboxID.fromString(
    Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join(""),
  );
  const newDefaultOptions = {
    ...options,
    storage: {
      ...options.storage,
      location: new sdk.StorageLocation.Default(),
      label: crypto.randomUUID(),
    },
  };
  const filesBeforeBuild = await poolFilenames();
  // verifies: STORE-007
  try {
    await sdk.Client.build(identity, newDefaultOptions, newInbox);
    throw new Error("build opened a database without a stored identity");
  } catch (error) {
    if (!(error instanceof sdk.XmtpError.IdentityNotFound)) throw error;
  }
  if (
    JSON.stringify(await poolFilenames()) !== JSON.stringify(filesBeforeBuild)
  )
    throw new Error("build created an OPFS database without a stored identity");
  try {
    sent.client();
    throw new Error("ended client remained in registry");
  } catch (error) {
    if (!(error instanceof sdk.XmtpError.ClientClosed)) throw error;
  }
  const reopened = await sdk.Client.build(identity, options, inboxID);
  if (reopened.inboxID().toString() !== inboxID.toString())
    throw new Error("inbox changed");
  const defaultOptions = {
    ...options,
    storage: {
      ...options.storage,
      location: new sdk.StorageLocation.Default(),
    },
  };
  const defaultClient = await sdk.Client.create(signer, defaultOptions);
  // verifies: STORE-005
  const defaultPath = await defaultClient.storage().path();
  if (!defaultPath?.startsWith("xmtp-sdk/"))
    throw new Error("browser default storage is outside xmtp-sdk");
  const defaultInbox = defaultClient.inboxID();
  await defaultClient.end();
  const reopenedDefault = await sdk.Client.build(
    identity,
    defaultOptions,
    defaultInbox,
  );
  if ((await reopenedDefault.storage().path()) !== defaultPath)
    throw new Error("browser default database did not reopen");
  await reopenedDefault.end();
  postMessage({ result: "Browser scenario 2 passed" });

  const liveGroup = await reopened.conversations().createGroup([], undefined);
  const reader = await liveGroup.messageReader();
  const liveID = await liveGroup.sendText("durable stream", undefined);
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
  const protocolGroup = await reopened
    .conversations()
    .createGroup([], undefined);
  const firstID = await protocolGroup.sendText("ack on request", undefined);
  const firstStream = new sdk.MessageStream(
    (signal) => protocolGroup.messageReader({ signal }),
    reopened,
  );
  if ((await firstStream.next()).value?.id.toString() !== firstID.toString())
    throw new Error("first adapter delivery missing");
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
        () => reject(new Error("adapter prefetched and acknowledged a value")),
        3_000,
      );
    }),
  ]).finally(() => clearTimeout(replayTimer));
  if (replayedItem.value?.id.toString() !== firstID.toString())
    throw new Error("adapter prefetched and acknowledged a value");
  const secondID = await protocolGroup.sendText("second request", undefined);
  if ((await secondStream.next()).value?.id.toString() !== secondID.toString())
    throw new Error("second adapter delivery missing");
  await secondStream.return();
  const afterAck = await protocolGroup.messageReader();
  if ((await afterAck.next())?.id.toString() !== secondID.toString())
    throw new Error("adapter did not acknowledge on next request");
  await afterAck.end();
  let resolveCreation!: (reader: {
    next: () => Promise<undefined>;
    end: () => Promise<void>;
  }) => void;
  let markCreationStarted!: () => void;
  const creationStarted = new Promise<void>((resolve) => {
    markCreationStarted = resolve;
  });
  let endedLate = false;
  const opening = new sdk.MessageStream(
    () =>
      new Promise((resolve) => {
        resolveCreation = resolve;
        markCreationStarted();
      }),
    reopened,
  );
  const openingRead = opening.next();
  await creationStarted;
  // Ending waits for the in-flight creation, then ends the late reader.
  const openingEnd = opening.return();
  resolveCreation({
    next: async () => undefined,
    end: async () => {
      endedLate = true;
    },
  });
  await openingEnd;
  if (!endedLate) throw new Error("late reader remained open");
  if (!(await openingRead).done)
    throw new Error("cancelled creation returned a value");
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
  if (!(await rejectedRead).done)
    throw new Error("cancelled creation rejected a read");
  await reopened.end();
  postMessage({ result: "Browser scenario 7 passed" });
}

run().then(
  () => postMessage({ result: "PASS" }),
  (error) =>
    postMessage({ result: `FAIL: ${String(error)}\n${error?.stack ?? ""}` }),
);
