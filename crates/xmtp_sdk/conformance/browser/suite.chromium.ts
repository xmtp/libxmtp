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
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { Client, Message } from "../../../../target/sdk-generated/typescript-wasm/index";
import { MainSession } from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/main/session";
import type {
  WireEndpoint,
  WireMessage,
} from "../../../../target/sdk-generated/typescript-wasm/runtime/bridge/wire";
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
  backendURL?: string,
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
        if (!backendURL) throw new Error("missing backend for reentry");
        const backend = await Backend.connect(session, {
          url: backendURL,
          appVersion: undefined,
          credentials: undefined,
          credential: undefined,
        });
        equal(
          backend.handle.type,
          "Backend",
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
    const mainSigner = signer(session, true, backendURL);
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
    expect(parent instanceof Message, "list message was not lifted to the host");
    expect(reply instanceof Message, "optional message was not lifted to the host");
    equal(
      (await reopened.decodeContent(parent.encoded)).tag,
      Pure.StandardContent_Tags.Text,
      "pure WASM did not decode the message",
    );
    equal(parent.content.tag, B.MessageContent_Tags.Text, "host content changed");
    if (parent.content.tag === B.MessageContent_Tags.Text)
      equal(parent.content.inner[0], "parent", "host text was not decoded");
    const hostReactionID = await parent.react({
      content: "host",
      action: B.ReactionAction.Added,
      schema: B.ReactionSchema.Unicode,
    });
    expect(hostReactionID.toString().length > 0, "host action did not send");
    expect(markdownID.toString().length > 0, "markdown was not sent");
    expect(receiptID.toString().length > 0, "read receipt was not sent");
    equal(
      parent.reactions[0]?.id.toString(),
      reactionID.toString(),
      "reaction missing",
    );
    equal(parent.replyCount, 1n, "reply count changed");
    equal(
      reply.inReplyTo?.id.toString(),
      parentID.toString(),
      "reply parent changed",
    );
    equal(reply.replyContent?.tag, B.MessageBody_Tags.Text, "reply body changed");
    if (reply.replyContent?.tag === B.MessageBody_Tags.Text)
      equal(reply.replyContent.inner[0], "reply", "reply body was not decoded");
    equal(
      reply.inReplyToContent?.tag,
      B.MessageBody_Tags.Text,
      "parent content changed",
    );
    if (reply.inReplyToContent?.tag === B.MessageBody_Tags.Text)
      equal(
        reply.inReplyToContent.inner[0],
        "parent",
        "parent body was not decoded",
      );
    expect((await parent.reply("host reply")).toString().length > 0, "host reply did not send");
    results.push("scenario 5: text, markdown, receipt, reaction, and reply");

    const customType = B.ContentTypeID.create({
      authorityID: "example.org",
      typeID: "bridge-conformance",
      versionMajor: 1,
      versionMinor: 0,
    });
    const customBytes = new TextEncoder().encode("custom browser value");
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
    const failingType = B.ContentTypeID.create({
      authorityID: "example.org",
      typeID: "bridge-failing",
      versionMajor: 1,
      versionMinor: 0,
    });
    const failingCodec = {
      type: failingType,
      encode(value: string): B.EncodedContent {
        return B.EncodedContent.create({
          type: failingType,
          content: new TextEncoder().encode(value).buffer,
        });
      },
      decode(): string {
        throw new Error("bad custom payload");
      },
    };
    const customOptions = {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.InMemory.new(),
      },
      codecs: [customCodec, failingCodec],
    };
    const customOwner = await Client.create(session, signer(session), customOptions);
    const customGroup = await customOwner.conversations().createGroup([], undefined);
    const customID = await customGroup.send(
      B.EncodedContent.create({
        type: customType,
        parameters: new Map([["source", "browser"]]),
        fallback: "custom",
        content: customBytes.buffer,
      }),
      undefined,
    );
    const custom = await customOwner.conversations().getMessageByID(customID);
    expect(custom, "custom message was not read");
    expect(custom instanceof Message, "custom message was not lifted to the host");
    equal(custom.encoded.fallback, "custom", "custom fallback was lost");
    equal(custom.encoded.parameters.get("source"), "browser", "map was lost");
    equal(new TextDecoder().decode(custom.encoded.content), "custom browser value", "custom bytes changed");
    equal(custom.content.tag, B.MessageContent_Tags.Custom, "custom tag changed");
    if (custom.content.tag === B.MessageContent_Tags.Custom)
      equal(custom.content.inner.value, "custom browser value", "host custom content failed");
    const customReplyID = await custom.reply(customCodec, "custom reply");
    const customReply = await customOwner.conversations().getMessageByID(customReplyID);
    expect(customReply instanceof Message, "custom reply was not lifted");
    equal(customReply.replyContent?.tag, B.MessageBody_Tags.Custom, "custom reply tag changed");
    if (customReply.replyContent?.tag === B.MessageBody_Tags.Custom)
      equal(customReply.replyContent.inner.value, "custom reply", "custom reply decode failed");
    const unknownType = B.ContentTypeID.create({
      authorityID: "example.org",
      typeID: "bridge-unknown",
      versionMajor: 1,
      versionMinor: 0,
    });
    const unknownID = await customGroup.send(
      B.EncodedContent.create({
        type: unknownType,
        content: new Uint8Array([1, 2, 3]).buffer,
      }),
      undefined,
    );
    const unknown = await customOwner.conversations().getMessageByID(unknownID);
    expect(unknown, "unknown content was not read");
    expect(unknown instanceof Message, "unknown content was not lifted");
    equal(unknown.content.tag, B.MessageContent_Tags.Custom, "unknown content tag changed");
    if (unknown.content.tag === B.MessageContent_Tags.Custom)
      equal(unknown.content.inner.value, undefined, "unknown codec produced a value");
    const failingID = await customGroup.send(failingCodec.encode("bad"), undefined);
    const failed = await customOwner.conversations().getMessageByID(failingID);
    expect(failed, "failed custom content was not read");
    expect(failed instanceof Message, "failed custom content was not lifted");
    if (failed.content.tag === B.MessageContent_Tags.Custom)
      expect(failed.content.inner.error?.includes("bad custom payload"), "codec error was lost");
    else throw new Error("failed custom content changed tag");
    await customOwner.end();
    results.push("scenario 6: custom codec registry, unknown codec, and error");

    const readerGroup = await reopened
      .conversations()
      .createGroup([], undefined);
    const reader = await readerGroup.messageReader();
    const next = reader.next();
    const readerID = await readerGroup.sendText("raw reader smoke", undefined);
    const nextMessage = await next;
    expect(nextMessage instanceof Message, "reader message was not lifted to the host");
    equal(
      nextMessage?.id.toString(),
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
    const catchUp = await reopened.catchUpToLive(10_000n);
    expect(typeof catchUp.completed === "boolean", "catch-up result is missing");
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
    await checkError(
      () => parent.react({ content: "closed", action: B.ReactionAction.Added, schema: B.ReactionSchema.Unicode }),
      (error) => B.XmtpError.ClientClosed.instanceOf(error),
      "message action did not fail after end",
    );
    await second.conversations().listGroups(undefined);
    await second.end();
    results.push("smoke: two page clients share the origin lock");

  } catch (error) {
    console.error(
      "browser stage",
      results.at(-1),
      error && typeof error === "object" ? Reflect.get(error, "inner") : error,
    );
    throw error;
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
