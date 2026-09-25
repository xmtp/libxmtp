// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { generatePrivateKey } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/generatePrivateKey.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { privateKeyToAccount } from "../../../../sdks/browser/node_modules/viem/_esm/accounts/privateKeyToAccount.js";
// @ts-ignore The browser fixture uses the published JavaScript build of viem.
import { toBytes } from "../../../../sdks/browser/node_modules/viem/_esm/utils/encoding/toBytes.js";
import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import {
  CONTRACT_HASH,
  PROTOCOL_VERSION,
} from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import {
  Backend,
  Client,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
import {
  Client as RuntimeClient,
  ClientRegistry,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/client";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";

function expect(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected)
    throw new Error(`${message}: ${String(actual)} != ${String(expected)}`);
}

function connection(hash = CONTRACT_HASH): {
  session: MainSession;
  worker: Worker;
} {
  const worker = new Worker(new URL("./suite.worker.ts", import.meta.url), {
    type: "module",
  });
  const endpoint: WireEndpoint = {
    postMessage(message, transfer) {
      worker.postMessage(message, { transfer });
    },
    onMessage(handler) {
      worker.addEventListener("message", (event: MessageEvent<WireMessage>) =>
        handler(event.data),
      );
    },
    onExit(handler) {
      worker.addEventListener("error", handler);
    },
    terminate() {
      worker.terminate();
    },
  };
  return { worker, session: new MainSession(endpoint, PROTOCOL_VERSION, hash) };
}

function signer(
  session: MainSession,
  reenter = false,
): {
  identity: () => Promise<B.PublicIdentity>;
  kind: () => Promise<B.SignerKind>;
  sign: (request: { text: string }) => Promise<B.Signature>;
  didReenter: () => boolean;
} {
  const account = privateKeyToAccount(generatePrivateKey());
  let reentered = false;
  return {
    async identity() {
      return {
        identifier: account.address.toLowerCase(),
        kind: B.PublicIdentityKind.Ethereum,
      };
    },
    async kind() {
      return B.SignerKind.Eoa.new();
    },
    async sign(request) {
      if (reenter) {
        equal(
          await session.call("__conformanceInner", []),
          "reentered",
          "signer callback could not call the SDK worker",
        );
        reentered = true;
      }
      const signed = await account.signMessage({ message: request.text });
      return B.Signature.Ecdsa.new(Uint8Array.from(toBytes(signed)).buffer);
    },
    didReenter: () => reentered,
  };
}

function options(
  path: string,
  backendURL: string,
  auto = true,
): B.ClientOptions {
  return {
    backend: B.BackendSource.Options.new({
      options: {
        url: backendURL,
        appVersion: undefined,
        credential: undefined,
        credentials: undefined,
      },
    }),
    storage: {
      location: B.StorageLocation.Path.new(path),
      label: path,
      encryptionKey: undefined,
      pool: undefined,
      singleConnection: false,
    },
    deviceSync: false,
    registration: { auto, nonce: undefined },
    forkRecovery: undefined,
    workers: undefined,
  };
}

async function checkError(
  action: () => Promise<unknown>,
  test: (error: Error) => boolean,
  message: string,
): Promise<void> {
  try {
    await action();
  } catch (error) {
    if (error instanceof Error && test(error)) return;
    throw error;
  }
  throw new Error(message);
}

export async function runBrowserBridgeConformance(
  backendURL: string,
): Promise<string[]> {
  const results: string[] = [];
  const { worker, session } = connection();
  let client: Client | undefined;
  let reopened: Client | undefined;
  try {
    await Pure.initPureWasm();
    expect(Pure.sdkVersion().startsWith("1.12.0"), "wrong SDK version");
    const pureText = Pure.encodeText("pure browser value");
    equal(
      Pure.decodeStandard(pureText).tag,
      Pure.StandardContent_Tags.Text,
      "main-thread pure WASM did not decode text",
    );
    await session.ready();
    expect(CONTRACT_HASH.length > 10, "missing contract checksum");
    results.push("scenario 1: pure WASM, worker WASM, version, and contract");

    // Create the signer before the first client. Its sign method calls the SDK
    // worker while Rust waits for the callback.
    const mainSigner = signer(session, true);
    const identity = await mainSigner.identity();
    const databasePath = `conformance-${crypto.randomUUID()}.db`;
    const clientOptions = options(databasePath, backendURL);
    client = await Client.create(session, mainSigner, clientOptions);
    expect(mainSigner.didReenter(), "signer did not reenter the SDK");
    const inboxID = client.inboxID();
    equal(await client.storage().path(), databasePath, "OPFS path changed");
    expect(client.libxmtpVersion().length > 0, "missing SDK version");
    await client.end();
    await checkError(
      async () => client?.conversations(),
      (error) =>
        B.XmtpError.ClientClosed.instanceOf(error) &&
        error.inner[0].code === "ClientClosed" &&
        error.inner[0].category === B.ErrorCategory.Lifecycle &&
        error.inner[0].retryable === false,
      "ended client accepted a call",
    );
    reopened = await Client.build(session, identity, clientOptions, inboxID);
    equal(reopened.inboxID().toString(), inboxID.toString(), "inbox changed");
    const firstGroup = await reopened
      .conversations()
      .createGroup([], undefined);
    const sentID = await firstGroup.sendText("bridge browser", undefined);
    expect(
      (await firstGroup.messages(undefined)).some(
        (message) => message.id.toString() === sentID.toString(),
      ),
      "sent message was not read from SQLite",
    );
    results.push("scenario 2: create, OPFS, reopen, end");
    results.push("smoke: OPFS database in worker");
    results.push("smoke: signer created before first client");
    results.push("smoke: signer callback reentered the SDK");

    const largeExpiry = 9_007_199_254_740_993n;
    let credentialCalls = 0;
    const credentialOptions: B.ClientOptions = {
      ...clientOptions,
      backend: B.BackendSource.Options.new({
        options: {
          url: backendURL,
          appVersion: undefined,
          credential: undefined,
          credentials: {
            async credential() {
              credentialCalls++;
              return {
                name: undefined,
                value: "Bearer test",
                expiresAtSeconds: largeExpiry,
              };
            },
          },
        },
      }),
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.InMemory.new(),
      },
    };
    const credentialClient = await Client.build(
      session,
      identity,
      credentialOptions,
      inboxID,
    );
    expect(credentialCalls > 0, "credential callback was not called");
    await credentialClient.setCredential({
      name: undefined,
      value: "Bearer refreshed",
      expiresAtSeconds: largeExpiry,
    });
    await credentialClient.end();
    results.push("scenario 3: credential callback and 64-bit expiry");

    const group = await reopened.conversations().createGroup([], {
      permissions: undefined,
      name: "browser family",
      imageUrl: undefined,
      description: undefined,
      disappearing: undefined,
      appData: undefined,
    });
    equal((await group.state()).name, "browser family", "group name changed");
    expect(
      (await reopened.conversations().listGroups(undefined)).some(
        (candidate) => candidate.id().toString() === group.id().toString(),
      ),
      "created group was not listed",
    );
    results.push("scenario 4: create and list a group");

    const parentID = await group.sendText("parent", undefined);
    const reactionID = await reopened.conversations().reactToMessage(
      parentID,
      {
        content: "ok",
        action: B.ReactionAction.Added,
        schema: B.ReactionSchema.Unicode,
      },
      undefined,
    );
    const replyID = await reopened
      .conversations()
      .replyToMessage(parentID, Pure.encodeText("reply"), undefined);
    const markdownID = await group.sendMarkdown("**markdown**", undefined);
    const receiptID = await group.sendReadReceipt(undefined);
    const parent = (await group.messages(undefined)).find(
      (message) => message.id.toString() === parentID.toString(),
    );
    const reply = await reopened.conversations().getMessageByID(replyID);
    expect(parent, "parent message was not read");
    expect(reply, "reply message was not read");
    expect(markdownID.toString().length > 0, "markdown was not sent");
    expect(receiptID.toString().length > 0, "read receipt was not sent");
    equal(
      parent.data.reactions[0]?.id.toString(),
      reactionID.toString(),
      "reaction missing",
    );
    equal(parent.data.replyCount, 1n, "reply count changed");
    equal(
      reply.data.inReplyTo?.id.toString(),
      parentID.toString(),
      "reply parent changed",
    );
    results.push("scenario 5: text, markdown, receipt, reaction, and reply");

    const customType = B.ContentTypeID.create({
      authorityID: "example.org",
      typeID: "bridge-conformance",
      versionMajor: 1,
      versionMinor: 0,
    });
    const customBytes = new TextEncoder().encode("custom browser value");
    const customID = await group.send(
      B.EncodedContent.create({
        type: customType,
        parameters: new Map([["source", "browser"]]),
        fallback: "custom",
        content: customBytes.buffer,
      }),
      undefined,
    );
    const custom = await reopened.conversations().getMessageByID(customID);
    expect(custom, "custom message was not read");
    equal(custom.encoded.fallback, "custom", "custom fallback was lost");
    equal(custom.encoded.parameters.get("source"), "browser", "map was lost");
    equal(
      new TextDecoder().decode(custom.encoded.content),
      "custom browser value",
      "custom bytes changed",
    );
    const customCodec = {
      type: customType,
      encode(value: string): B.EncodedContent {
        return B.EncodedContent.create({
          type: customType,
          content: new TextEncoder().encode(value).buffer,
        });
      },
      decode(value: B.EncodedContent): string {
        return new TextDecoder().decode(value.content);
      },
    };
    // The public runtime constructor normally receives the raw binding from
    // Client.create. The bridge test supplies its generated proxy instead.
    const owner = Reflect.construct(RuntimeClient, [
      reopened,
      [customCodec],
    ]) as RuntimeClient;
    equal(
      owner.decodeCustom(custom.encoded)?.value,
      "custom browser value",
      "custom codec failed",
    );
    equal(
      ClientRegistry.get(reopened.clientKey()),
      owner,
      "codec owner changed",
    );
    ClientRegistry.delete(reopened.clientKey());
    const unknownOwner = Reflect.construct(RuntimeClient, [
      reopened,
      [],
    ]) as RuntimeClient;
    equal(
      unknownOwner.decodeCustom(custom.encoded),
      undefined,
      "unknown codec decoded a value",
    );
    const failingOwner = Reflect.construct(RuntimeClient, [
      reopened,
      [
        {
          ...customCodec,
          decode(): string {
            throw new Error("custom decode failed");
          },
        },
      ],
    ]) as RuntimeClient;
    expect(
      failingOwner
        .decodeCustom(custom.encoded)
        ?.error?.includes("custom decode failed"),
      "decode error was lost",
    );
    ClientRegistry.delete(reopened.clientKey());
    results.push("scenario 6: custom codec registry, unknown codec, and error");

    const readerGroup = await reopened
      .conversations()
      .createGroup([], undefined);
    const reader = await readerGroup.messageReader();
    const next = reader.next();
    const readerID = await readerGroup.sendText("raw reader smoke", undefined);
    equal(
      (await next)?.id.toString(),
      readerID.toString(),
      "raw reader missed the message",
    );
    await reader.end();
    results.push(
      "PENDING scenario 7 and stream smoke: raw reader passed; Task 19 ack adapter is on #4250 (O2)",
    );
    results.push(
      "PENDING scenario 8: Task 20 events and listeners are on #4251 (O2)",
    );

    const key = new Uint8Array(32).fill(7).buffer;
    const archive = await reopened.archives().exportToBytes(key, undefined);
    expect(archive.byteLength > 0, "archive bytes were empty");
    equal(
      (await reopened.archives().metadataFromBytes(archive, key)).backupVersion,
      0,
      "archive metadata changed",
    );
    results.push("scenario 9: archive bytes");

    const config = reopened.serverConfiguration();
    equal(
      (await reopened.refreshServerConfiguration()).identifier,
      config.identifier,
      "server configuration changed",
    );
    const backend = await Backend.connect(session, {
      url: backendURL,
      appVersion: undefined,
      credentials: undefined,
      credential: undefined,
    });
    equal(backend.handle.type, "Backend", "backend handle type changed");
    await checkError(
      () =>
        Client.create(session, signer(session), {
          ...clientOptions,
          storage: {
            ...clientOptions.storage,
            location: B.StorageLocation.Default.new(),
          },
        }),
      (error) => B.XmtpError.StorageLocationRequired.instanceOf(error),
      "missing typed storage error",
    );
    results.push("scenario 10: configuration and typed error");

    const unsignedSigner = signer(session);
    const unsigned = await Client.create(session, unsignedSigner, {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.InMemory.new(),
      },
      registration: { auto: false, nonce: undefined },
    });
    equal(await unsigned.isRegistered(), false, "new client was registered");
    const request = await unsigned.unsafeCreateInboxSignatureRequest();
    expect(request, "signature request was not created");
    expect(
      (await request.signatureText()).length > 0,
      "signature text was empty",
    );
    await request.sign(unsignedSigner);
    await unsigned.unsafeApplySignatureRequest(request);
    equal(await unsigned.isRegistered(), true, "signature was not applied");
    await unsigned.end();
    results.push("scenario 11: signature request through worker");

    const second = await Client.build(
      session,
      identity,
      {
        ...clientOptions,
        storage: {
          ...clientOptions.storage,
          location: B.StorageLocation.Path.new(
            `second-${crypto.randomUUID()}.db`,
          ),
        },
      },
      inboxID,
    );
    await reopened.end();
    reopened = undefined;
    await second.conversations().listGroups(undefined);
    await second.end();
    results.push("smoke: two page clients share the origin lock");

    const waiting = session.call("__conformanceWait", []);
    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    void session.call("__conformanceCrash", []).catch(() => {});
    await checkError(
      () => waiting,
      (error) => Reflect.get(error, "code") === "workerTerminated",
      "worker death left a call pending",
    );
    results.push("smoke: real WASM worker death settles pending calls");
  } finally {
    if (reopened) await reopened.end();
    worker.terminate();
  }

  const refused = connection("wrong-contract-hash");
  try {
    await checkError(
      () => refused.session.ready(),
      (error) => Reflect.get(error, "code") === "contractMismatch",
      "contract mismatch was accepted",
    );
    results.push("smoke: contract mismatch is refused before calls");
  } finally {
    refused.worker.terminate();
  }
  return results;
}
