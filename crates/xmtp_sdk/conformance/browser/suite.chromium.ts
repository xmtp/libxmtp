import * as Pure from "../../../../target/sdk-generated/typescript-pure/index";
import { CONTRACT_HASH } from "../../../../target/sdk-generated/typescript-wasm/contract.gen";
import {
  Client,
  ConversationStream,
  EventStream,
  Message,
  MessageStream,
} from "../../../../target/sdk-generated/typescript-wasm/index";
import { Backend } from "../../../../target/sdk-generated/typescript-wasm/proxy.gen";
import * as B from "../../../../target/sdk-generated/typescript-wasm/xmtp_sdk";
import {
  expect,
  equal,
  connection,
  signer,
  options,
  checkError,
  checkRejectedPromise,
} from "./suite-support";

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
    const inboxId = client.inboxId();
    equal(
      await client.isRegistered(),
      true,
      "created client was not registered",
    );
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
    reopened = await Client.build(session, identity, clientOptions, inboxId);
    equal(reopened.inboxId().toString(), inboxId.toString(), "inbox changed");
    await checkError(
      // Uppercase hex decodes, so only ID validation rejects it.
      () => reopened!.conversations().getMessageById("AB".repeat(32)),
      (error) =>
        B.XmtpError.InvalidArgument.instanceOf(error) &&
        error.inner[0].code === "InvalidArgument" &&
        error.inner[0].category === B.ErrorCategory.Input &&
        error.inner[0].retryable === false,
      "browser worker accepted a malformed message ID",
    );
    const firstGroup = await reopened
      .conversations()
      .createGroup([], undefined);
    const sentId = await firstGroup.sendText("bridge browser", undefined);
    expect(
      (await firstGroup.messages(undefined)).some(
        (message) => message.id.toString() === sentId.toString(),
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
    const credentialClient = await Client.create(
      session,
      signer(session),
      credentialOptions,
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

    const parentId = await group.sendText("parent", undefined);
    const reactionId = await reopened.conversations().reactToMessage(
      parentId,
      {
        content: "ok",
        action: B.ReactionAction.Added,
        schema: B.ReactionSchema.Unicode,
      },
      undefined,
    );
    const replyId = await reopened
      .conversations()
      .replyToMessage(parentId, Pure.encodeText("reply"), undefined);
    const markdownId = await group.sendMarkdown("**markdown**", undefined);
    const receiptId = await group.sendReadReceipt(undefined);
    const parent = (await group.messages(undefined)).find(
      (message) => message.id.toString() === parentId.toString(),
    );
    const reply = await reopened.conversations().getMessageById(replyId);
    expect(parent, "parent message was not read");
    expect(reply, "reply message was not read");
    expect(
      parent instanceof Message,
      "list message was not lifted to the host",
    );
    expect(
      reply instanceof Message,
      "optional message was not lifted to the host",
    );
    equal(
      (await reopened.decodeContent(parent.encoded)).tag,
      Pure.StandardContent_Tags.Text,
      "pure WASM did not decode the message",
    );
    equal(
      parent.content.tag,
      B.MessageContent_Tags.Text,
      "host content changed",
    );
    if (parent.content.tag === B.MessageContent_Tags.Text)
      equal(parent.content.inner[0], "parent", "host text was not decoded");
    const hostReactionId = await parent.react({
      content: "host",
      action: B.ReactionAction.Added,
      schema: B.ReactionSchema.Unicode,
    });
    expect(hostReactionId.toString().length > 0, "host action did not send");
    expect(markdownId.toString().length > 0, "markdown was not sent");
    expect(receiptId.toString().length > 0, "read receipt was not sent");
    equal(
      parent.reactions[0]?.id.toString(),
      reactionId.toString(),
      "reaction missing",
    );
    equal(parent.replyCount, 1n, "reply count changed");
    equal(
      reply.inReplyTo?.id.toString(),
      parentId.toString(),
      "reply parent changed",
    );
    equal(
      reply.replyContent?.tag,
      B.MessageBody_Tags.Text,
      "reply body changed",
    );
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
    expect(
      (await parent.reply("host reply")).toString().length > 0,
      "host reply did not send",
    );
    results.push("scenario 5: text, markdown, receipt, reaction, and reply");

    const customType = B.ContentTypeId.create({
      authorityId: "example.org",
      typeId: "bridge-conformance",
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
    const failingType = B.ContentTypeId.create({
      authorityId: "example.org",
      typeId: "bridge-failing",
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
    const collisionRegisteredType = B.ContentTypeId.create({
      authorityId: "example.org",
      typeId: "a/b",
      versionMajor: 1,
      versionMinor: 0,
    });
    const collisionOtherType = B.ContentTypeId.create({
      authorityId: "example.org/a",
      typeId: "b",
      versionMajor: 1,
      versionMinor: 0,
    });
    const collisionCodec = {
      type: collisionRegisteredType,
      encode(value: string): B.EncodedContent {
        return B.EncodedContent.create({
          type: collisionRegisteredType,
          content: new TextEncoder().encode(value).buffer,
        });
      },
      decode(): string {
        return "wrong collision codec";
      },
    };
    const customOptions = {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.InMemory.new(),
      },
      codecs: [customCodec, failingCodec, collisionCodec],
    };
    const customOwner = await Client.create(
      session,
      signer(session),
      customOptions,
    );
    const customGroup = await customOwner
      .conversations()
      .createGroup([], undefined);
    const customId = await customGroup.send(
      B.EncodedContent.create({
        type: customType,
        parameters: new Map([["source", "browser"]]),
        fallback: "custom",
        content: customBytes.buffer,
      }),
      undefined,
    );
    const custom = await customOwner.conversations().getMessageById(customId);
    expect(custom, "custom message was not read");
    expect(
      custom instanceof Message,
      "custom message was not lifted to the host",
    );
    equal(custom.encoded.fallback, "custom", "custom fallback was lost");
    equal(custom.encoded.parameters.get("source"), "browser", "map was lost");
    equal(
      new TextDecoder().decode(custom.encoded.content),
      "custom browser value",
      "custom bytes changed",
    );
    equal(
      custom.content.tag,
      B.MessageContent_Tags.Custom,
      "custom tag changed",
    );
    if (custom.data.content.tag !== B.MessageContent_Tags.Custom)
      throw new Error("stored custom content changed tag");
    if (custom.content.tag === B.MessageContent_Tags.Custom) {
      equal(
        custom.content.inner.value,
        "custom browser value",
        "host custom content failed",
      );
      expect(
        custom.content.inner.rawBytes.byteLength > 0,
        "custom raw bytes were empty",
      );
      equal(
        new Uint8Array(custom.content.inner.rawBytes).toString(),
        new Uint8Array(custom.data.content.inner.rawBytes).toString(),
        "custom raw bytes changed",
      );
    }
    const customReplyId = await custom.reply(customCodec, "custom reply");
    const customReply = await customOwner
      .conversations()
      .getMessageById(customReplyId);
    expect(customReply instanceof Message, "custom reply was not lifted");
    equal(
      customReply.replyContent?.tag,
      B.MessageBody_Tags.Custom,
      "custom reply tag changed",
    );
    if (customReply.replyContent?.tag === B.MessageBody_Tags.Custom)
      equal(
        customReply.replyContent.inner.value,
        "custom reply",
        "custom reply decode failed",
      );
    const alternateCodec = {
      ...customCodec,
      decode(): string {
        return "other client";
      },
    };
    const alternateOwner = await Client.create(session, signer(session), {
      ...customOptions,
      codecs: [alternateCodec],
    });
    const alternateGroup = await alternateOwner
      .conversations()
      .createGroup([], undefined);
    const alternateId = await alternateGroup.send(
      customCodec.encode("same type"),
      undefined,
    );
    const alternate = await alternateOwner
      .conversations()
      .getMessageById(alternateId);
    expect(
      alternate instanceof Message,
      "second client's message was not lifted",
    );
    if (alternate.content.tag === B.MessageContent_Tags.Custom)
      equal(
        alternate.content.inner.value,
        "other client",
        "codec leaked across clients",
      );
    else throw new Error("second client's custom content changed tag");
    const originalAgain = await customOwner
      .conversations()
      .getMessageById(customId);
    expect(
      originalAgain instanceof Message,
      "first client's message was not lifted",
    );
    if (originalAgain.content.tag === B.MessageContent_Tags.Custom)
      equal(
        originalAgain.content.inner.value,
        "custom browser value",
        "first codec was replaced",
      );
    else throw new Error("first client's custom content changed tag");
    await alternateOwner.end();
    const unknownType = B.ContentTypeId.create({
      authorityId: "example.org",
      typeId: "bridge-unknown",
      versionMajor: 1,
      versionMinor: 0,
    });
    const unknownId = await customGroup.send(
      B.EncodedContent.create({
        type: unknownType,
        content: new Uint8Array([1, 2, 3]).buffer,
      }),
      undefined,
    );
    const unknown = await customOwner.conversations().getMessageById(unknownId);
    expect(unknown, "unknown content was not read");
    expect(unknown instanceof Message, "unknown content was not lifted");
    equal(
      unknown.content.tag,
      B.MessageContent_Tags.Unknown,
      "unknown content tag changed",
    );
    if (unknown.data.content.tag !== B.MessageContent_Tags.Custom)
      throw new Error("stored unknown content changed tag");
    if (unknown.content.tag === B.MessageContent_Tags.Unknown) {
      expect(
        unknown.content.inner.rawBytes.byteLength > 0,
        "unknown raw bytes were empty",
      );
      equal(
        new Uint8Array(unknown.content.inner.rawBytes).toString(),
        new Uint8Array(unknown.data.content.inner.rawBytes).toString(),
        "unknown raw bytes changed",
      );
    }
    const unknownReplyId = await unknown.reply(
      B.EncodedContent.create({
        type: unknownType,
        content: new Uint8Array([4]).buffer,
      }),
      undefined,
    );
    const unknownReply = await customOwner
      .conversations()
      .getMessageById(unknownReplyId);
    expect(unknownReply instanceof Message, "unknown reply was not lifted");
    equal(
      unknownReply.replyContent?.tag,
      B.MessageBody_Tags.Unknown,
      "unknown reply body changed tag",
    );
    const failingId = await customGroup.send(
      failingCodec.encode("bad"),
      undefined,
    );
    const failed = await customOwner.conversations().getMessageById(failingId);
    expect(failed, "failed custom content was not read");
    expect(failed instanceof Message, "failed custom content was not lifted");
    if (failed.content.tag === B.MessageContent_Tags.Custom)
      expect(
        failed.content.inner.error?.includes("bad custom payload"),
        "codec error was lost",
      );
    else throw new Error("failed custom content changed tag");
    const collisionId = await customGroup.send(
      B.EncodedContent.create({
        type: collisionOtherType,
        content: new Uint8Array([9]).buffer,
      }),
      undefined,
    );
    const collision = await customOwner
      .conversations()
      .getMessageById(collisionId);
    expect(collision, "colliding content was not read");
    expect(collision instanceof Message, "colliding content was not lifted");
    equal(
      collision.content.tag,
      B.MessageContent_Tags.Unknown,
      "colliding content changed tag",
    );
    if (collision.content.tag === B.MessageContent_Tags.Unknown)
      expect(
        collision.content.inner.rawBytes.byteLength > 0,
        "colliding raw bytes were empty",
      );
    await customOwner.end();
    const closedMessage = new Message(custom.data, session);
    equal(
      closedMessage.content.tag,
      B.MessageContent_Tags.Custom,
      "closed client's content changed tag",
    );
    if (closedMessage.content.tag === B.MessageContent_Tags.Custom) {
      equal(
        closedMessage.content.inner.error,
        "clientClosed",
        "closed client error was lost",
      );
      expect(
        closedMessage.content.inner.rawBytes.byteLength > 0,
        "closed client raw bytes were empty",
      );
    }
    results.push("scenario 6: custom codec registry, unknown codec, and error");

    const readerGroup = await reopened
      .conversations()
      .createGroup([], undefined);
    const reader = await readerGroup.messageReader();
    const next = reader.next();
    const readerId = await readerGroup.sendText("raw reader smoke", undefined);
    const nextMessage = await next;
    expect(
      nextMessage instanceof Message,
      "reader message was not lifted to the host",
    );
    equal(
      nextMessage?.id.toString(),
      readerId.toString(),
      "raw reader missed the message",
    );
    await reader.end();
    const closeReasons: string[] = [];
    const connectionStates: B.ConnectionState[] = [];
    const messageStream = new MessageStream(
      (signal) => readerGroup.messageReader({ signal }),
      reopened,
      {
        onClose: (reason) => closeReasons.push(reason.kind),
        onConnectionStateChange: (_previous, current) =>
          connectionStates.push(current),
      },
    );
    equal(
      (await messageStream.next()).value?.id.toString(),
      readerId.toString(),
      "reader did not replay its unacknowledged message",
    );
    // The first state is the one read at subscription, which the
    // bridge reports asynchronously.
    for (let i = 0; i < 100 && connectionStates.length === 0; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    expect(
      connectionStates[0] === B.ConnectionState.Connected ||
        connectionStates[0] === B.ConnectionState.Connecting,
      "reader did not report its connection state",
    );
    await messageStream.return();
    equal(closeReasons.join(), "closed", "reader did not call onClose");
    const replay = new MessageStream(
      (signal) => readerGroup.messageReader({ signal }),
      reopened,
    );
    equal(
      (await replay.next()).value?.id.toString(),
      readerId.toString(),
      "closing the stream acknowledged the last message",
    );
    const pendingMessage = replay.next();
    const nextId = await readerGroup.sendText("next request", undefined);
    equal(
      (await pendingMessage).value?.id.toString(),
      nextId.toString(),
      "next request did not receive the new message",
    );
    const idleRead = replay.next();
    await replay.return();
    equal((await idleRead).done, true, "idle read did not cancel");
    const conversationStream = ConversationStream.openBrowser(reopened, {
      consentStates: [B.ConsentState.Unknown, B.ConsentState.Allowed],
    });
    await conversationStream.ready();
    const denied = await reopened.conversations().createGroup([], undefined);
    await reopened.preferences().setConsentStates([
      {
        entity: B.ConsentEntity.Conversation.new({
          conversationId: denied.id(),
        }),
        state: B.ConsentState.Denied,
      },
    ]);
    const allowed = await reopened.conversations().createGroup([], undefined);
    const selected = (await conversationStream.next()).value;
    equal(
      selected?.tag,
      B.Conversation_Tags.Group,
      "conversation reader tag changed",
    );
    if (selected?.tag === B.Conversation_Tags.Group)
      equal(
        selected.inner.group.id().toString(),
        allowed.id().toString(),
        "conversation reader did not apply consentStates",
      );
    await conversationStream.end();
    results.push(
      "scenario 7: message and conversation readers, consentStates, connection state, onClose, and idle cancellation",
    );

    const eventFilter: B.EventFilter = {
      kinds: [B.EventKind.ConversationJoined],
      conversationIds: undefined,
      contentTypes: undefined,
      referencesOwnMessages: false,
    };
    const eventReader = await reopened.events(eventFilter);
    let listenerCalls = 0;
    const listenerId = await reopened.startListener(eventFilter, {
      async onEvent(event) {
        equal(
          event.tag,
          B.ClientEvent_Tags.ConversationJoined,
          "listener event changed",
        );
        listenerCalls += 1;
      },
    });
    await reopened.conversations().createGroup([], undefined);
    equal(
      (await eventReader.next())?.tag,
      B.ClientEvent_Tags.ConversationJoined,
      "event reader missed the join",
    );
    for (let attempt = 0; attempt < 100 && listenerCalls === 0; attempt += 1)
      await new Promise<void>((resolve) => setTimeout(resolve, 10));
    equal(listenerCalls, 1, "listener missed the join");
    await reopened.stopListener(listenerId);
    await eventReader.end();
    const eventStream = new EventStream(await reopened.events(eventFilter));
    await reopened.conversations().createGroup([], undefined);
    equal(
      (await eventStream.next()).value?.tag,
      B.ClientEvent_Tags.ConversationJoined,
      "event stream missed the join",
    );
    await eventStream.return();
    results.push("scenario 8: event reader, listener, stop, and event stream");

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
    // A peer adds this client to a new group and sends to it. This client
    // has not synced, so the Welcome and the messages are outstanding work
    // that only catch-up can process.
    const peer = await Client.create(session, signer(session), {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.InMemory.new(),
      },
    });
    const owedGroup = await peer
      .conversations()
      .createGroup([reopened.inboxId()], undefined);
    const owed = ["owed 0", "owed 1", "owed 2"];
    for (const text of owed) await owedGroup.sendText(text, undefined);
    equal(
      await reopened.conversations().getById(owedGroup.id()),
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
    const joined = await reopened.conversations().getById(owedGroup.id());
    if (joined?.tag !== B.Conversation_Tags.Group)
      throw new Error("catch-up did not store the new group");
    const received = (await joined.inner.group.messages(undefined)).flatMap(
      (message) => {
        expect(message instanceof Message, "caught-up message was not lifted");
        return message.content.tag === B.MessageContent_Tags.Text
          ? [message.content.inner[0]]
          : [];
      },
    );
    equal(received.join(), owed.join(), "catch-up did not store the messages");
    const again = await reopened.catchUpToLive(10_000n);
    equal(again.completed, true, "second catch-up did not complete");
    equal(again.conversations, 0n, "second catch-up counted the group again");
    equal(again.messages, 0n, "second catch-up counted the messages again");
    await peer.end();
    const backend = await Backend.connect(session, {
      url: backendURL,
      appVersion: undefined,
      credentials: undefined,
      credential: undefined,
    });
    equal(backend.handle.type, "Backend", "backend handle type changed");
    const sameText = "1111111111111111111111111111111111111111";
    const mixedIdentities = [
      { identifier: sameText, kind: B.PublicIdentityKind.Ethereum },
      { identifier: sameText, kind: B.PublicIdentityKind.Passkey },
      identity,
    ];
    const checkMixedCanMessage = (result: Map<string, boolean>) => {
      equal(result.size, 3, "canMessage lost an identity kind");
      equal(result.get(`ethereum:${sameText}`), false, "Ethereum value changed");
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
    const defaultOptions = {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.Default.new(),
      },
    };
    const defaultClient = await Client.create(
      session,
      defaultSigner,
      defaultOptions,
    );
    const defaultInboxId = defaultClient.inboxId();
    const defaultPath = await defaultClient.storage().path();
    expect(
      defaultPath?.startsWith("xmtp-sdk/"),
      "default browser database is outside xmtp-sdk/",
    );
    await defaultClient.end();
    const reopenedDefault = await Client.build(
      session,
      defaultIdentity,
      defaultOptions,
      defaultInboxId,
    );
    equal(
      await reopenedDefault.storage().path(),
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
        const opened = await Client.create(session, signer(session), {
          ...keyOptions,
          storage: {
            ...keyOptions.storage,
            location: B.StorageLocation.InMemory.new(),
            encryptionKey: new Uint8Array(32).fill(7).buffer,
          },
        } as B.ClientOptions);
        await opened.end();
      },
      (error) =>
        B.XmtpError.InvalidInput.instanceOf(error) &&
        error.inner[0].code === "InvalidInput" &&
        error.inner[0].category === B.ErrorCategory.Input,
      "browser accepted an encryption key",
    );
    results.push("scenario 10: catch-up, configuration, and default storage");

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

    const second = await Client.create(session, mainSigner, {
      ...clientOptions,
      storage: {
        ...clientOptions.storage,
        location: B.StorageLocation.Path.new(
          `second-${crypto.randomUUID()}.db`,
        ),
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
          const opened = await Client.create(
            other.session,
            signer(other.session),
            options(`other-${crypto.randomUUID()}.db`, backendURL),
          );
          await opened.end();
        },
        (error) =>
          B.XmtpError.StorageBusy.instanceOf(error) &&
          error.inner[0].message === "storageBusy",
        "a second worker opened OPFS storage under the origin lock",
      );
    } finally {
      other.worker.terminate();
    }
    await reopened.end();
    reopened = undefined;
    const closedReaction = {
      content: "closed",
      action: B.ReactionAction.Added,
      schema: B.ReactionSchema.Unicode,
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
    await second.conversations().listGroups(undefined);
    await second.end();
    const staleClient = new Client(session, second.handle);
    try {
      await checkRejectedPromise(() => staleClient.end(), "Client.end");
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
