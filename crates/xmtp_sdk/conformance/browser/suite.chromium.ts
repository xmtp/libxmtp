import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import { CONTRACT_HASH } from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import { Message as HostMessage } from "../../../../target/sdk-generated/typescript-wasm/host-message.gen";
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
// These scenarios run clients in their own worker session, so they can check
// the transport. Each client is used through the public layer over the
// session's worker proxy; `Client` here is the proxy, only for the stale-handle
// check at the end.
import {
  Backend as ProxyBackend,
  Client,
} from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import { currentProjection } from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
import { boundMessage } from "../../../../target/sdk-generated/typescript-wasm/runtime/public/message";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";
import { controlPackageWorker } from "./attachment-worker-control";
import { within } from "./attachments-support";
import {
  build,
  checkError,
  checkRejectedPromise,
  connection,
  create,
  equal,
  expect,
  isPublicError,
  options,
  signer,
} from "./suite-support";

// Text encoded on the main thread by the pure module, as a public value.
function text(value: string): sdk.EncodedContent {
  return Pure.encodeText(value);
}

export async function runBrowserBridgeConformance(
  backendURL: string,
): Promise<string[]> {
  const results: string[] = [];
  const { worker, session } = connection();
  let client: sdk.Client | undefined;
  let reopened: sdk.Client | undefined;
  try {
    await Pure.initPureWasm();
    expect(Pure.sdkVersion().startsWith("1.12.0"), "wrong SDK version");
    const pureText = Pure.encodeText("pure browser value");
    equal(
      Pure.decodeStandard(pureText).kind,
      "text",
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
    client = (await create(session, mainSigner, clientOptions)).client;
    expect(mainSigner.didReenter(), "signer did not reenter the SDK");
    const inboxId = client.inboxId;
    equal(
      await client.isRegistered(),
      true,
      "created client was not registered",
    );
    equal(await client.storage.path(), databasePath, "OPFS path changed");
    expect(client.libxmtpVersion.length > 0, "missing SDK version");
    await client.end();
    await checkError(
      async () => client?.conversations.listGroups(undefined),
      (error) =>
        isPublicError(error, sdk.XmtpError.ClientClosed, "lifecycle") &&
        (error as sdk.XmtpError).details.code === "ClientClosed" &&
        (error as sdk.XmtpError).details.retryable === false,
      "ended client accepted a call",
    );
    reopened = (await build(session, identity, clientOptions, inboxId)).client;
    equal(reopened.inboxId, inboxId, "inbox changed");
    await checkError(
      // Uppercase hex decodes, so only ID validation rejects it.
      () => reopened!.conversations.getMessageById("AB".repeat(32)),
      (error) =>
        isPublicError(error, sdk.XmtpError.InvalidArgument, "input") &&
        (error as sdk.XmtpError).details.code === "InvalidArgument" &&
        (error as sdk.XmtpError).details.retryable === false,
      "browser worker accepted a malformed message ID",
    );
    const firstGroup = await reopened.conversations.createGroup([]);
    const sentId = await firstGroup.sendText("bridge browser");
    expect(
      (await firstGroup.messages()).some((message) => message.id === sentId),
      "sent message was not read from SQLite",
    );
    results.push("scenario 2: create, OPFS, reopen, end");
    results.push("smoke: OPFS database in worker");
    results.push("smoke: signer created before first client");
    results.push("smoke: signer callback reentered the SDK");

    const largeExpiry = 9_007_199_254_740_993n;
    let credentialCalls = 0;
    const credentialOptions: sdk.ClientOptions = {
      ...clientOptions,
      backend: {
        url: backendURL,
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
      storage: { ...clientOptions.storage, location: "inMemory" },
    };
    const credentialClient = (
      await create(session, signer(session), credentialOptions)
    ).client;
    expect(credentialCalls > 0, "credential callback was not called");
    await credentialClient.setCredential({
      name: undefined,
      value: "Bearer refreshed",
      expiresAtSeconds: largeExpiry,
    });
    await credentialClient.end();
    results.push("scenario 3: credential callback and 64-bit expiry");

    const group = await reopened.conversations.createGroup([], {
      name: "browser family",
    });
    equal((await group.state()).name, "browser family", "group name changed");
    expect(
      (await reopened.conversations.listGroups(undefined)).some(
        (candidate) => candidate.id === group.id,
      ),
      "created group was not listed",
    );
    results.push("scenario 4: create and list a group");

    const parentId = await group.sendText("parent");
    const reactionId = await reopened.conversations.reactToMessage(parentId, {
      content: "ok",
      action: "added",
      schema: "unicode",
    });
    const replyId = await reopened.conversations.replyToMessage(
      parentId,
      text("reply"),
    );
    const markdownId = await group.sendMarkdown("**markdown**");
    const receiptId = await group.sendReadReceipt();
    const parent = (await group.messages()).find(
      (message) => message.id === parentId,
    );
    const reply = await reopened.conversations.getMessageById(replyId);
    expect(parent, "parent message was not read");
    expect(reply, "reply message was not read");
    expect(parent instanceof sdk.Message, "list message was not public");
    expect(reply instanceof sdk.Message, "optional message was not public");
    expect(parent.encoded, "text codec input was not retained");
    equal(
      (await reopened.decodeContent(parent.encoded)).kind,
      "text",
      "the worker did not decode the message",
    );
    equal(parent.content.kind, "text", "host content changed");
    if (parent.content.kind === "text")
      equal(parent.content.value, "parent", "host text was not decoded");
    const hostReactionId = await parent.react({
      content: "host",
      action: "added",
      schema: "unicode",
    });
    expect(hostReactionId.toString().length > 0, "host action did not send");
    expect(markdownId.toString().length > 0, "markdown was not sent");
    expect(receiptId.toString().length > 0, "read receipt was not sent");
    equal(parent.reactions[0]?.id, reactionId, "reaction missing");
    equal(parent.replyCount, 1n, "reply count changed");
    equal(reply.inReplyTo?.id, parentId, "reply parent changed");
    equal(reply.replyContent?.kind, "text", "reply body changed");
    if (reply.replyContent?.kind === "text")
      equal(reply.replyContent.value, "reply", "reply body was not decoded");
    equal(reply.inReplyToContent?.kind, "text", "parent content changed");
    if (reply.inReplyToContent?.kind === "text")
      equal(
        reply.inReplyToContent.value,
        "parent",
        "parent body was not decoded",
      );
    expect(
      (await parent.reply("host reply")).toString().length > 0,
      "host reply did not send",
    );
    results.push("scenario 5: text, markdown, receipt, reaction, and reply");

    const typeOf = (typeId: string, authorityId = "example.org") =>
      ({
        authorityId,
        typeId,
        versionMajor: 1,
        versionMinor: 0,
      }) satisfies sdk.ContentTypeId;
    const encodedOf = (
      type: sdk.ContentTypeId,
      content: Uint8Array,
      extra: Partial<sdk.EncodedContent> = {},
    ): sdk.EncodedContent => ({
      type,
      parameters: new Map(),
      content,
      ...extra,
    });
    const customType = typeOf("bridge-conformance");
    const customBytes = new TextEncoder().encode("custom browser value");
    const customCodec: sdk.ContentCodec<string> = {
      type: customType,
      encode: (value) => encodedOf(customType, new TextEncoder().encode(value)),
      decode: (value) => new TextDecoder().decode(value.content),
    };
    const failingType = typeOf("bridge-failing");
    const failingCodec: sdk.ContentCodec<string> = {
      type: failingType,
      encode: (value) =>
        encodedOf(failingType, new TextEncoder().encode(value)),
      decode(encoded) {
        const value = new TextDecoder().decode(encoded.content);
        if (value === "null prototype") throw Object.create(null);
        if (value === "throwing toString")
          throw {
            toString() {
              throw new Error("diagnostic failed");
            },
          };
        throw new Error("bad custom payload");
      },
    };
    const collisionRegisteredType = typeOf("a/b");
    const collisionOtherType = typeOf("b", "example.org/a");
    const collisionCodec: sdk.ContentCodec<string> = {
      type: collisionRegisteredType,
      encode: (value) =>
        encodedOf(collisionRegisteredType, new TextEncoder().encode(value)),
      decode: () => "wrong collision codec",
    };
    const customOptions: sdk.ClientOptions = {
      ...clientOptions,
      storage: { ...clientOptions.storage, location: "inMemory" },
      codecs: [customCodec, failingCodec, collisionCodec],
    };
    const customOwner = (await create(session, signer(session), customOptions))
      .client;
    const customGroup = await customOwner.conversations.createGroup([]);
    const customId = await customGroup.send(
      encodedOf(customType, customBytes, {
        parameters: new Map([["source", "browser"]]),
        fallback: "custom",
      }),
    );
    const custom = await customOwner.conversations.getMessageById(customId);
    expect(custom instanceof sdk.Message, "custom message was not public");
    equal(custom.encoded!.fallback, "custom", "custom fallback was lost");
    equal(custom.encoded!.parameters?.get("source"), "browser", "map was lost");
    equal(
      new TextDecoder().decode(custom.encoded!.content),
      "custom browser value",
      "custom bytes changed",
    );
    if (custom.content.kind !== "custom")
      throw new Error("custom content changed kind");
    equal(custom.content.value, "custom browser value", "custom decode failed");
    expect(custom.content.rawBytes.byteLength > 0, "custom raw bytes empty");
    // The public raw bytes are the stored bytes of the binding message.
    const storedContent = boundMessage(custom).data.content;
    if (storedContent.tag !== B.MessageContent_Tags.Custom)
      throw new Error("stored custom content changed kind");
    equal(
      custom.content.rawBytes.toString(),
      new Uint8Array(storedContent.inner.rawBytes).toString(),
      "custom raw bytes changed",
    );
    const customReplyId = await custom.reply(customCodec, "custom reply");
    const customReply =
      await customOwner.conversations.getMessageById(customReplyId);
    expect(customReply instanceof sdk.Message, "custom reply was not public");
    if (customReply.replyContent?.kind !== "custom")
      throw new Error("custom reply body changed kind");
    equal(
      customReply.replyContent.value,
      "custom reply",
      "custom reply decode failed",
    );
    const alternateCodec: sdk.ContentCodec<string> = {
      ...customCodec,
      decode: () => "other client",
    };
    const alternateOwner = (
      await create(session, signer(session), {
        ...customOptions,
        codecs: [alternateCodec],
      })
    ).client;
    const alternateGroup = await alternateOwner.conversations.createGroup([]);
    const alternateId = await alternateGroup.send(
      customCodec.encode("same type"),
    );
    const alternate =
      await alternateOwner.conversations.getMessageById(alternateId);
    expect(alternate instanceof sdk.Message, "second client's message");
    if (alternate.content.kind !== "custom")
      throw new Error("second client's custom content changed kind");
    equal(
      alternate.content.value,
      "other client",
      "codec leaked across clients",
    );
    const originalAgain =
      await customOwner.conversations.getMessageById(customId);
    expect(originalAgain instanceof sdk.Message, "first client's message");
    if (originalAgain.content.kind !== "custom")
      throw new Error("first client's custom content changed kind");
    equal(
      originalAgain.content.value,
      "custom browser value",
      "first codec was replaced",
    );
    await alternateOwner.end();
    const unknownType = typeOf("bridge-unknown");
    const unknownId = await customGroup.send(
      encodedOf(unknownType, new Uint8Array([1, 2, 3])),
    );
    const unknown = await customOwner.conversations.getMessageById(unknownId);
    expect(unknown instanceof sdk.Message, "unknown content was not public");
    if (unknown.content.kind !== "unknown")
      throw new Error("unknown content changed kind");
    // The unknown item keeps the original serialized bytes from Rust, the
    // same bytes that a decoded custom item keeps.
    expect(unknown.content.rawBytes.byteLength > 0, "unknown raw bytes empty");
    expect(
      unknown.content.rawBytes.byteLength > unknown.encoded!.content.byteLength,
      "unknown raw bytes are not the serialized envelope",
    );
    const unknownReplyId = await unknown.reply(
      encodedOf(unknownType, new Uint8Array([4])),
    );
    const unknownReply =
      await customOwner.conversations.getMessageById(unknownReplyId);
    expect(unknownReply instanceof sdk.Message, "unknown reply not public");
    equal(
      unknownReply.replyContent?.kind,
      "unknown",
      "unknown reply body changed kind",
    );
    const failingId = await customGroup.send(failingCodec.encode("bad"));
    const failed = await customOwner.conversations.getMessageById(failingId);
    expect(failed instanceof sdk.Message, "failed custom content not public");
    if (failed.content.kind !== "custom")
      throw new Error("failed custom content changed kind");
    expect(
      failed.content.error?.message.includes("bad custom payload"),
      "codec error was lost",
    );
    const collisionId = await customGroup.send(
      encodedOf(collisionOtherType, new Uint8Array([9])),
    );
    const collision =
      await customOwner.conversations.getMessageById(collisionId);
    expect(collision instanceof sdk.Message, "colliding content not public");
    if (collision.content.kind !== "unknown")
      throw new Error("colliding content changed kind");
    expect(
      collision.content.rawBytes.byteLength > 0,
      "colliding raw bytes were empty",
    );
    // verifies: PROC-045, CTYPE-009
    const codecStream = sdk.MessageStream.openGroup(customOwner, customGroup);
    const streamFailureId = await customGroup.send(
      failingCodec.encode("stream failure"),
    );
    const streamGoodId = await customGroup.send(
      customCodec.encode("after codec failure"),
    );
    let sawCodecFailure = false;
    for (;;) {
      const item = (await codecStream.next()).value;
      expect(
        item instanceof sdk.Message,
        "codec stream ended before a valid item",
      );
      if (item.id === streamFailureId) {
        expect(
          item.content.kind === "custom",
          "failed codec stream item lost custom content",
        );
        if (item.content.kind !== "custom")
          throw new Error("custom stream failure missing");
        equal(
          item.content.error?.code,
          "CodecDecodeFailed",
          "codec stream error code",
        );
        equal(
          item.content.error?.category,
          "callback",
          "codec stream error category",
        );
        expect(
          item.content.rawBytes.byteLength > 0,
          "codec stream lost failed bytes",
        );
        sawCodecFailure = true;
      }
      if (item.id === streamGoodId) {
        expect(
          item.content.kind === "custom",
          "next custom stream item changed kind",
        );
        if (item.content.kind !== "custom")
          throw new Error("next custom stream item missing");
        equal(
          item.content.value,
          "after codec failure",
          "stream stopped after codec failure",
        );
        break;
      }
    }
    expect(sawCodecFailure, "codec stream hid the failed content item");
    for (const hostile of ["null prototype", "throwing toString"]) {
      const failedId = await customGroup.send(failingCodec.encode(hostile));
      const failed = (await codecStream.next()).value;
      expect(failed instanceof sdk.Message, "hostile failure ended the stream");
      equal(failed.id, failedId, "hostile failure item missing");
      if (failed.content.kind !== "custom")
        throw new Error("hostile failure lost custom content");
      equal(
        failed.content.error?.code,
        "CodecDecodeFailed",
        "hostile error code",
      );
      equal(
        failed.content.error?.category,
        "callback",
        "hostile error category",
      );
      equal(failed.content.error?.retryable, false, "hostile error retry flag");
      equal(
        failed.content.error?.message,
        "custom content codec failed",
        "hostile diagnostic fallback",
      );
      equal(
        failed.content.value,
        undefined,
        "hostile failure produced a value",
      );
      expect(
        failed.content.rawBytes.byteLength > 0,
        "hostile failure lost raw bytes",
      );
      const goodId = await customGroup.send(
        customCodec.encode("after hostile failure"),
      );
      const good = (await codecStream.next()).value;
      expect(
        good instanceof sdk.Message,
        "valid item after hostile failure missing",
      );
      equal(good.id, goodId, "valid item after hostile failure missing");
      if (good.content.kind !== "custom")
        throw new Error("valid item after hostile failure lost custom content");
      equal(
        good.content.value,
        "after hostile failure",
        "stream stopped after hostile failure",
      );
    }
    console.log(
      "Browser stream delivered both hostile codec failures and the next valid items",
    );
    await codecStream.return();
    await customOwner.end();
    // A Message of an ended client keeps its fields; its actions fail.
    equal(custom.content.value, "custom browser value", "content changed");
    await checkRejectedPromise(() => custom.refresh(), "Message.refresh");
    // Host level: a message lifted after its client ended keeps its bytes and
    // reports `clientClosed` for custom content.
    const closedMessage = new HostMessage(boundMessage(custom).data, session);
    equal(
      closedMessage.content.tag,
      B.MessageContent_Tags.Custom,
      "closed client's content changed tag",
    );
    if (closedMessage.content.tag === B.MessageContent_Tags.Custom) {
      equal(
        closedMessage.content.inner.error?.code,
        "ClientClosed",
        "closed client error was lost",
      );
      expect(
        closedMessage.content.inner.rawBytes.byteLength > 0,
        "closed client raw bytes were empty",
      );
    }
    results.push("scenario 6: custom codec registry, unknown codec, and error");

    const readerGroup = await reopened.conversations.createGroup([]);
    const reader = await readerGroup.messageReader();
    const next = reader.next();
    const readerId = await readerGroup.sendText("raw reader smoke");
    const nextMessage = await next;
    expect(nextMessage instanceof sdk.Message, "reader message was not public");
    equal(nextMessage.id, readerId, "raw reader missed the message");
    await reader.end();
    const closeReasons: string[] = [];
    const connectionStates: sdk.ConnectionState[] = [];
    const messageStream = sdk.MessageStream.openGroup(
      reopened,
      readerGroup,
      undefined,
      {
        onClose: (reason) => closeReasons.push(reason.kind),
        onConnectionStateChange: (_previous, current) =>
          connectionStates.push(current),
      },
    );
    equal(
      (await messageStream.next()).value?.id,
      readerId,
      "reader did not replay its unacknowledged message",
    );
    // The first state is the one read at subscription, which the
    // bridge reports asynchronously.
    for (let i = 0; i < 100 && connectionStates.length === 0; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    expect(
      connectionStates[0] === "connected" ||
        connectionStates[0] === "connecting",
      "reader did not report a public connection state",
    );
    await messageStream.return();
    equal(closeReasons.join(), "closed", "reader did not call onClose");
    const replay = sdk.MessageStream.openGroup(reopened, readerGroup);
    equal(
      (await replay.next()).value?.id,
      readerId,
      "closing the stream acknowledged the last message",
    );
    const pendingMessage = replay.next();
    const nextId = await readerGroup.sendText("next request");
    equal(
      (await pendingMessage).value?.id,
      nextId,
      "next request did not receive the new message",
    );
    const idleRead = replay.next();
    await replay.return();
    equal((await idleRead).done, true, "idle read did not cancel");
    const conversationStream = sdk.ConversationStream.open(reopened, {
      consentStates: ["unknown", "allowed"],
    });
    await conversationStream.ready();
    const denied = await reopened.conversations.createGroup([]);
    await reopened.preferences.setConsentStates([
      {
        entity: { kind: "conversation", conversationId: denied.id },
        state: "denied",
      },
    ]);
    const allowed = await reopened.conversations.createGroup([]);
    const selected = (await conversationStream.next()).value;
    expect(selected instanceof sdk.Group, "conversation reader gave no Group");
    equal(
      selected.id,
      allowed.id,
      "conversation reader did not apply consentStates",
    );
    await conversationStream.end();
    results.push(
      "scenario 7: message and conversation readers, consentStates, connection state, onClose, and idle cancellation",
    );

    const eventFilter: sdk.EventFilter = {
      kinds: ["conversation.joined"],
      referencesOwnMessages: false,
    };
    const eventReader = await reopened.events(eventFilter);
    let listenerCalls = 0;
    const listenerId = await reopened.startListener(eventFilter, (event) => {
      equal(event.kind, "conversation.joined", "listener event changed");
      listenerCalls += 1;
    });
    await reopened.conversations.createGroup([]);
    equal(
      (await eventReader.next()).value?.kind,
      "conversation.joined",
      "event stream missed the join",
    );
    for (let attempt = 0; attempt < 100 && listenerCalls === 0; attempt += 1)
      await new Promise<void>((resolve) => setTimeout(resolve, 10));
    equal(listenerCalls, 1, "listener missed the join");
    await reopened.stopListener(listenerId);
    await eventReader.return();
    const eventStream = await reopened.events(eventFilter);
    await reopened.conversations.createGroup([]);
    equal(
      (await eventStream.next()).value?.kind,
      "conversation.joined",
      "event stream missed the join",
    );
    await eventStream.return();
    results.push("scenario 8: event reader, listener, stop, and event stream");

    const key = new Uint8Array(32).fill(7);
    const archive = await reopened.archives.exportToBytes(key, undefined);
    expect(
      archive instanceof Uint8Array && archive.byteLength > 0,
      "archive bytes were empty",
    );
    equal(
      (await reopened.archives.metadataFromBytes(archive, key)).backupVersion,
      0,
      "archive metadata changed",
    );
    results.push("scenario 9: archive bytes");

    const config = reopened.serverConfiguration;
    equal(
      (await reopened.refreshServerConfiguration()).identifier,
      config.identifier,
      "server configuration changed",
    );
    // A peer adds this client to a new group and sends to it. This client
    // has not synced, so the Welcome and the messages are outstanding work
    // that only catch-up can process.
    const peer = (
      await create(session, signer(session), {
        ...clientOptions,
        storage: { ...clientOptions.storage, location: "inMemory" },
      })
    ).client;
    const owedGroup = await peer.conversations.createGroup([reopened.inboxId]);
    const owed = ["owed 0", "owed 1", "owed 2"];
    for (const value of owed) await owedGroup.sendText(value);
    equal(
      await reopened.conversations.getById(owedGroup.id),
      undefined,
      "the group was known before catch-up",
    );
    // verifies: PROC-016
    // Strict bigint and boolean comparisons also check the field types.
    const catchUp = await reopened.catchUpToLive(10_000n);
    equal(catchUp.completed, true, "catch-up did not complete");
    equal(catchUp.conversations, 1n, "catch-up did not count the new group");
    expect(
      typeof catchUp.messages === "bigint" &&
        catchUp.messages >= BigInt(owed.length),
      `catch-up counted ${catchUp.messages} messages`,
    );
    equal(catchUp.failed, 0n, "catch-up reported a failed group");
    const joined = await reopened.conversations.getById(owedGroup.id);
    if (!(joined instanceof sdk.Group))
      throw new Error("catch-up did not store the new group");
    const received = (await joined.messages()).flatMap((message) => {
      expect(message instanceof sdk.Message, "caught-up message not public");
      return message.content.kind === "text" ? [message.content.value] : [];
    });
    equal(received.join(), owed.join(), "catch-up did not store the messages");
    const again = await reopened.catchUpToLive(10_000n);
    equal(again.completed, true, "second catch-up did not complete");
    equal(again.conversations, 0n, "second catch-up counted the group again");
    equal(again.messages, 0n, "second catch-up counted the messages again");
    await peer.end();
    // The public constructor runs in the package worker, not this session.
    const backendControl = controlPackageWorker();
    try {
      const backend = await sdk.Backend.connect({ url: backendURL });
      expect(backend instanceof sdk.Backend, "Backend.connect failed");
      expect(!("handle" in backend), "the public Backend exposes a handle");
      // The transport fixture releases its root without waiting for collection.
      const backendProxy = currentProjection().lowerBackend(backend);
      expect(backendProxy instanceof ProxyBackend, "not a worker Backend");
      backendProxy.release();
      await within(backendControl.terminated, "Backend worker termination");
    } finally {
      backendControl.restore();
    }
    const sameText = "1111111111111111111111111111111111111111";
    const mixedIdentities: sdk.PublicIdentity[] = [
      { identifier: sameText, kind: "ethereum" },
      { identifier: sameText, kind: "passkey" },
      identity,
    ];
    const checkMixedCanMessage = (result: Map<string, boolean>) => {
      equal(result.size, 3, "canMessage lost an identity kind");
      equal(
        result.get(`ethereum:${sameText}`),
        false,
        "Ethereum value changed",
      );
      equal(result.get(`passkey:${sameText}`), false, "passkey value changed");
      equal(
        result.get(`ethereum:${identity.identifier}`),
        true,
        "registered Ethereum value changed",
      );
    };
    checkMixedCanMessage(await reopened.canMessage(mixedIdentities));
    // verifies: STORE-005
    const defaultSigner = signer(session);
    const defaultIdentity = await defaultSigner.identity();
    const defaultOptions: sdk.ClientOptions = {
      ...clientOptions,
      storage: { ...clientOptions.storage, location: "default" },
    };
    const defaultClient = (await create(session, defaultSigner, defaultOptions))
      .client;
    const defaultInboxId = defaultClient.inboxId;
    const defaultPath = await defaultClient.storage.path();
    expect(
      defaultPath?.startsWith("xmtp-sdk/"),
      "default browser database is outside xmtp-sdk/",
    );
    await defaultClient.end();
    const reopenedDefault = (
      await build(session, defaultIdentity, defaultOptions, defaultInboxId)
    ).client;
    equal(
      await reopenedDefault.storage.path(),
      defaultPath,
      "default browser database path changed on reopen",
    );
    await reopenedDefault.end();
    const keyOptions = options(
      `key-${crypto.randomUUID()}.db`,
      backendURL,
      false,
    );
    await checkError(
      async () => {
        const opened = await create(session, signer(session), {
          ...keyOptions,
          storage: {
            ...keyOptions.storage,
            location: "inMemory",
            encryptionKey: new Uint8Array(32).fill(7),
          },
        } as sdk.ClientOptions);
        await opened.client.end();
      },
      (error) => isPublicError(error, sdk.XmtpError.InvalidInput, "input"),
      "browser accepted an encryption key",
    );
    results.push("scenario 10: catch-up, configuration, and default storage");

    const unsignedSigner = signer(session);
    const unsigned = (
      await create(session, unsignedSigner, {
        ...clientOptions,
        storage: { ...clientOptions.storage, location: "inMemory" },
        registration: { auto: false },
      })
    ).client;
    equal(await unsigned.isRegistered(), false, "new client was registered");
    const request = await unsigned.unsafeCreateInboxSignatureRequest();
    expect(request instanceof sdk.SignatureRequest, "no signature request");
    expect(
      (await request.signatureText()).length > 0,
      "signature text was empty",
    );
    await request.sign(unsignedSigner);
    await unsigned.unsafeApplySignatureRequest(request);
    equal(await unsigned.isRegistered(), true, "signature was not applied");
    await unsigned.end();
    results.push("scenario 11: signature request through worker");

    const secondId = crypto.randomUUID();
    const second = await create(session, mainSigner, {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: {
          dbPath: `second-${secondId}.db`,
          attachmentsDir: `second-${secondId}-attachments`,
        },
      },
    });
    // A second worker stands in for a second tab. While this worker holds
    // the origin lock, the other worker must not open the OPFS pool. SAH
    // contention in WASM is also StorageBusy, but only the lock refusal
    // carries the bridge code as its message.
    const other = connection();
    try {
      await other.session.ready();
      await checkError(
        async () => {
          const opened = await create(
            other.session,
            signer(other.session),
            options(`other-${crypto.randomUUID()}.db`, backendURL),
          );
          await opened.client.end();
        },
        (error) =>
          isPublicError(error, sdk.XmtpError.StorageBusy, "storage") &&
          (error as sdk.XmtpError).details.message === "storageBusy",
        "a second worker opened OPFS storage under the origin lock",
      );
    } finally {
      other.worker.terminate();
    }
    await reopened.end();
    reopened = undefined;
    const closedReaction: sdk.Reaction = {
      content: "closed",
      action: "added",
      schema: "unicode",
    };
    for (const [label, action] of [
      ["Message.delete", () => parent.delete()],
      ["Message.deleteLocally", () => parent.deleteLocally()],
      ["Message.react", () => parent.react(closedReaction)],
      ["Message.reply", () => parent.reply("closed")],
      ["Message.conversation", () => parent.conversation()],
    ] as const) {
      await checkRejectedPromise(action, label);
    }
    await second.client.conversations.listGroups(undefined);
    await second.client.end();
    // Transport: a stale proxy handle of the ended client is refused.
    const staleClient = new Client(session, second.proxy.handle);
    try {
      // The proxy is below the public layer, so its error is the binding one.
      await checkError(
        () => staleClient.end(),
        (error) => B.XmtpError.ClientClosed.instanceOf(error),
        "a stale proxy handle was accepted",
      );
    } finally {
      staleClient.release();
    }
    results.push(
      "smoke: two clients share a worker's lock; a second worker is StorageBusy",
    );
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
      (error) => Reflect.get(error, "code") === "ContractMismatch",
      "contract mismatch was accepted",
    );
    results.push("smoke: contract mismatch is refused before calls");
  } finally {
    refused.worker.terminate();
  }
  return results;
}
