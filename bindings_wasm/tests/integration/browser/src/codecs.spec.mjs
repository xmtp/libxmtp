import { expect, test, describe } from "vitest";
import init, {
  // Codecs
  TextCodec,
  ReactionCodec,
  ReplyCodec,
  ReadReceiptCodec,
  AttachmentCodec,
  RemoteAttachmentCodec,
  MultiRemoteAttachmentCodec,
  TransactionReferenceCodec,
  WalletSendCallsCodec,
  ActionsCodec,
  IntentCodec,
  GroupUpdatedCodec,
  LeaveRequestCodec,
  // Test helpers
  createTestClient,
} from "@xmtp/wasm-bindings";

await init();

describe("Codecs", () => {
  test("should encode and decode text", () => {
    const contentType = TextCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("text");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const text = "Hello, world!";
    const encoded = TextCodec.encode(text);
    expect(encoded.fallback).toEqual(undefined);
    const decoded = TextCodec.decode(encoded);
    expect(decoded).toEqual(text);
    expect(TextCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode reactions", () => {
    const contentType = ReactionCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("reaction");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const reaction = {
      reference: "123",
      referenceInboxId: "456",
      action: "added",
      content: "👍",
      schema: "unicode",
    };
    const encoded = ReactionCodec.encode(reaction);
    expect(encoded.fallback).toEqual(`Reacted with "👍" to an earlier message`);
    const decoded = ReactionCodec.decode(encoded);
    expect(decoded.reference).toEqual("123");
    expect(decoded.referenceInboxId).toEqual("456");
    expect(decoded.content).toEqual("👍");
    expect(decoded.schema).toEqual("unicode");
    expect(ReactionCodec.shouldPush()).toBe(false);

    const reaction2 = {
      reference: "123",
      referenceInboxId: "456",
      action: "removed",
      content: "👍",
      schema: "unicode",
    };
    const encoded2 = ReactionCodec.encode(reaction2);
    expect(encoded2.fallback).toEqual(`Removed "👍" from an earlier message`);
  });

  test("should encode and decode replies", () => {
    const contentType = ReplyCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("reply");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const innerContent = TextCodec.encode("Hello, world!");
    const reply = {
      content: innerContent,
      reference: "123",
      referenceInboxId: "456",
    };
    const encoded = ReplyCodec.encode(reply);
    expect(encoded.fallback).toEqual(
      `Replied with "Hello, world!" to an earlier message`
    );
    const decoded = ReplyCodec.decode(encoded);
    expect(decoded.reference).toEqual("123");
    expect(decoded.referenceInboxId).toEqual("456");
    expect(ReplyCodec.shouldPush()).toBe(true);

    const attachmentContent = AttachmentCodec.encode({
      mimeType: "image/png",
      content: new Uint8Array(),
    });
    const reply2 = {
      content: attachmentContent,
      reference: "123",
    };
    const encoded2 = ReplyCodec.encode(reply2);
    expect(encoded2.fallback).toEqual(`Replied to an earlier message`);
    expect(encoded2.referenceInboxId).toEqual(undefined);
  });

  test("should encode and decode read receipts", () => {
    const contentType = ReadReceiptCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("readReceipt");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const encoded = ReadReceiptCodec.encode();
    expect(encoded.fallback).toEqual(undefined);
    const decoded = ReadReceiptCodec.decode(encoded);
    expect(decoded).toMatchObject({});
    expect(ReadReceiptCodec.shouldPush()).toBe(false);
  });

  test("should encode and decode attachments", () => {
    const contentType = AttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("attachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const attachment = {
      filename: "attachment.png",
      mimeType: "image/png",
      content: new Uint8Array([1, 2, 3, 4, 5]),
    };
    const encoded = AttachmentCodec.encode(attachment);
    expect(encoded.fallback).toEqual(
      `Can't display attachment.png. This app doesn't support attachments.`
    );
    const decoded = AttachmentCodec.decode(encoded);
    expect(decoded.filename).toEqual("attachment.png");
    expect(decoded.mimeType).toEqual("image/png");
    expect(Array.from(decoded.content)).toEqual([1, 2, 3, 4, 5]);
    expect(AttachmentCodec.shouldPush()).toBe(true);

    const attachment2 = {
      mimeType: "application/pdf",
      content: new Uint8Array([1, 2, 3, 4, 5]),
    };
    const encoded2 = AttachmentCodec.encode(attachment2);
    expect(encoded2.fallback).toEqual(
      `Can't display this content. This app doesn't support attachments.`
    );
  });

  test("should encode and decode remote attachments", () => {
    const contentType = RemoteAttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("remoteStaticAttachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const remoteAttachment = {
      url: "https://example.com/attachment.png",
      contentDigest: "abc123",
      secret: new Uint8Array(32),
      salt: new Uint8Array(32),
      nonce: new Uint8Array(12),
      scheme: "https",
      contentLength: 100,
      filename: "attachment.png",
    };
    const encoded = RemoteAttachmentCodec.encode(remoteAttachment);
    expect(encoded.fallback).toEqual(
      `Can't display attachment.png. This app doesn't support remote attachments.`
    );
    const decoded = RemoteAttachmentCodec.decode(encoded);
    expect(decoded.url).toEqual("https://example.com/attachment.png");
    expect(decoded.contentDigest).toEqual("abc123");
    expect(decoded.scheme).toEqual("https");
    expect(decoded.contentLength).toEqual(100);
    expect(decoded.filename).toEqual("attachment.png");
    expect(RemoteAttachmentCodec.shouldPush()).toBe(true);

    const remoteAttachment2 = {
      url: "https://example.com/attachment.pdf",
      contentDigest: "def456",
      secret: new Uint8Array(32),
      salt: new Uint8Array(32),
      nonce: new Uint8Array(12),
      scheme: "https",
      contentLength: 200,
    };
    const encoded2 = RemoteAttachmentCodec.encode(remoteAttachment2);
    expect(encoded2.fallback).toEqual(
      `Can't display this content. This app doesn't support remote attachments.`
    );
  });

  test("should encode and decode multi remote attachments", () => {
    const contentType = MultiRemoteAttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("multiRemoteStaticAttachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const multiRemoteAttachment = {
      attachments: [
        {
          secret: new Uint8Array([1, 2, 3]),
          contentDigest: "123",
          nonce: new Uint8Array([4, 5, 6]),
          scheme: "https",
          url: "https://example.com/attachment1.png",
          salt: new Uint8Array([7, 8, 9]),
          contentLength: 100,
          filename: "attachment1.png",
        },
        {
          secret: new Uint8Array([10, 11, 12]),
          contentDigest: "456",
          nonce: new Uint8Array([13, 14, 15]),
          scheme: "https",
          url: "https://example.com/attachment2.pdf",
          salt: new Uint8Array([16, 17, 18]),
        },
      ],
    };
    const encoded = MultiRemoteAttachmentCodec.encode(multiRemoteAttachment);
    expect(encoded.fallback).toEqual(
      "Can't display this content. This app doesn't support multiple remote attachments."
    );
    const decoded = MultiRemoteAttachmentCodec.decode(encoded);
    expect(decoded.attachments.length).toEqual(2);
    expect(decoded.attachments[0].url).toEqual(
      "https://example.com/attachment1.png"
    );
    expect(decoded.attachments[0].filename).toEqual("attachment1.png");
    expect(decoded.attachments[0].contentDigest).toEqual("123");
    expect(decoded.attachments[0].contentLength).toEqual(100);
    expect(decoded.attachments[0].secret).toEqual(new Uint8Array([1, 2, 3]));
    expect(decoded.attachments[0].salt).toEqual(new Uint8Array([7, 8, 9]));
    expect(decoded.attachments[0].nonce).toEqual(new Uint8Array([4, 5, 6]));
    expect(decoded.attachments[0].scheme).toEqual("https");
    expect(decoded.attachments[1].url).toEqual(
      "https://example.com/attachment2.pdf"
    );
    expect(decoded.attachments[1].filename).toEqual(undefined);
    expect(decoded.attachments[1].contentDigest).toEqual("456");
    expect(decoded.attachments[1].contentLength).toEqual(undefined);
    expect(decoded.attachments[1].secret).toEqual(new Uint8Array([10, 11, 12]));
    expect(decoded.attachments[1].salt).toEqual(new Uint8Array([16, 17, 18]));
    expect(decoded.attachments[1].nonce).toEqual(new Uint8Array([13, 14, 15]));
    expect(decoded.attachments[1].scheme).toEqual("https");
    expect(MultiRemoteAttachmentCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode transaction reference", () => {
    const contentType = TransactionReferenceCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("transactionReference");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const transactionReference = {
      namespace: "eip155",
      networkId: "1",
      reference: "0xabc123",
    };
    const encoded = TransactionReferenceCodec.encode(transactionReference);
    expect(encoded.fallback).toEqual(
      `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: 0xabc123`
    );
    const decoded = TransactionReferenceCodec.decode(encoded);
    expect(decoded.namespace).toEqual("eip155");
    expect(decoded.networkId).toEqual("1");
    expect(decoded.reference).toEqual("0xabc123");
    expect(decoded.metadata).toBe(undefined);
    expect(TransactionReferenceCodec.shouldPush()).toBe(true);

    const transactionReference2 = {
      networkId: "1",
      reference: "",
      metadata: {
        transactionType: "transfer",
        currency: "ETH",
        amount: 1,
        decimals: 18,
        fromAddress: "0x123",
        toAddress: "0x456",
      },
    };
    const encoded2 = TransactionReferenceCodec.encode(transactionReference2);
    expect(encoded2.fallback).toEqual(`Crypto transaction`);
    const decoded2 = TransactionReferenceCodec.decode(encoded2);
    expect(decoded2.networkId).toEqual("1");
    expect(decoded2.reference).toEqual("");
    expect(decoded2.namespace).toBe(undefined);
    expect(decoded2.metadata).toMatchObject({
      transactionType: "transfer",
      currency: "ETH",
      amount: 1,
      decimals: 18,
      fromAddress: "0x123",
      toAddress: "0x456",
    });
  });

  test("should encode and decode wallet send calls", () => {
    const contentType = WalletSendCallsCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("walletSendCalls");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const walletSendCalls = {
      version: "1",
      chainId: "1",
      from: "0x123",
      calls: [
        {
          to: "0x456",
          data: "0x789",
          value: "0x0",
          gas: "0x5208",
          metadata: {
            description: "Send funds",
            transactionType: "transfer",
            extra: {
              note: "test",
            },
          },
        },
      ],
      capabilities: {
        foo: "bar",
      },
    };
    const encoded = WalletSendCallsCodec.encode(walletSendCalls);
    const decoded = WalletSendCallsCodec.decode(encoded);
    expect(decoded.version).toEqual("1");
    expect(decoded.chainId).toEqual("1");
    expect(decoded.from).toEqual("0x123");
    expect(decoded.calls.length).toEqual(1);
    expect(decoded.calls[0].to).toEqual("0x456");
    expect(WalletSendCallsCodec.shouldPush()).toBe(true);

    const walletSendCalls2 = {
      version: "1",
      chainId: "1",
      from: "0x123",
      calls: [],
    };
    const encoded2 = WalletSendCallsCodec.encode(walletSendCalls2);
    const decoded2 = WalletSendCallsCodec.decode(encoded2);
    expect(decoded2.calls.length).toBe(0);
    expect(decoded2.capabilities).toBe(undefined);
  });

  test("should encode and decode actions", () => {
    const contentType = ActionsCodec.contentType();
    expect(contentType.authorityId).toEqual("coinbase.com");
    expect(contentType.typeId).toEqual("actions");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const actions = {
      id: "1",
      description: "Test actions",
      expiresAtNs: 1_000_000n,
      actions: [
        {
          id: "1",
          label: "Test action",
          imageUrl: "https://example.com/image.png",
          expiresAtNs: 1_000_000n,
        },
        {
          id: "2",
          label: "Test action 2",
          style: "primary",
          expiresAtNs: 1_000_000n,
        },
      ],
    };
    const encoded = ActionsCodec.encode(actions);
    expect(encoded.fallback).toEqual(
      `Test actions\n\n[1] Test action\n[2] Test action 2\n\nReply with the number to select`
    );
    const decoded = ActionsCodec.decode(encoded);
    expect(decoded.id).toEqual("1");
    expect(decoded.description).toEqual("Test actions");
    expect(decoded.expiresAtNs).toEqual(1_000_000n);
    expect(decoded.actions.length).toEqual(2);
    expect(decoded.actions[0].label).toEqual("Test action");
    expect(decoded.actions[0].imageUrl).toEqual(
      "https://example.com/image.png"
    );
    expect(decoded.actions[0].expiresAtNs).toEqual(1_000_000n);
    expect(decoded.actions[1].label).toEqual("Test action 2");
    expect(decoded.actions[1].style).toEqual("primary");
    expect(decoded.actions[1].expiresAtNs).toEqual(1_000_000n);
    expect(ActionsCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode intents", () => {
    const contentType = IntentCodec.contentType();
    expect(contentType.authorityId).toEqual("coinbase.com");
    expect(contentType.typeId).toEqual("intent");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const intent = {
      id: "1",
      actionId: "action-1",
      metadata: {
        foo: "bar",
        count: 42,
        enabled: true,
        nullValue: null,
      },
    };
    const encoded = IntentCodec.encode(intent);
    expect(encoded.fallback).toEqual(`User selected action: action-1`);
    const decoded = IntentCodec.decode(encoded);
    expect(decoded.id).toEqual("1");
    expect(decoded.actionId).toEqual("action-1");
    expect(decoded.metadata.foo).toEqual("bar");
    expect(decoded.metadata.count).toEqual(42);
    expect(decoded.metadata.enabled).toEqual(true);
    expect(decoded.metadata.nullValue).toEqual(null);
    expect(IntentCodec.shouldPush()).toBe(true);

    const intent2 = {
      id: "2",
      actionId: "action-2",
    };
    const encoded2 = IntentCodec.encode(intent2);
    expect(encoded2.fallback).toEqual(`User selected action: action-2`);
    const decoded2 = IntentCodec.decode(encoded2);
    expect(decoded2.id).toEqual("2");
    expect(decoded2.actionId).toEqual("action-2");
  });

  test("should decode group updated", async () => {
    const contentType = GroupUpdatedCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("group_updated");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);

    const alix = await createTestClient();
    const bo = await createTestClient();

    const group = await alix
      .conversations()
      .createGroupByInboxIds([bo.inboxId]);

    const messages = await group.findMessages();
    const groupUpdated = GroupUpdatedCodec.decode(messages[0].content);
    expect(groupUpdated.initiatedByInboxId).toEqual(alix.inboxId);
    expect(groupUpdated.addedInboxes.length).toEqual(1);
    expect(groupUpdated.addedInboxes[0].inboxId).toEqual(bo.inboxId);
    expect(groupUpdated.removedInboxes.length).toEqual(0);
    expect(groupUpdated.metadataFieldChanges.length).toEqual(0);
    expect(groupUpdated.leftInboxes.length).toEqual(0);
    expect(groupUpdated.addedAdminInboxes.length).toEqual(0);
    expect(groupUpdated.addedSuperAdminInboxes.length).toEqual(0);
    expect(groupUpdated.removedAdminInboxes.length).toEqual(0);
    expect(groupUpdated.removedSuperAdminInboxes.length).toEqual(0);
  });

  test("should decode leave request", async () => {
    const contentType = LeaveRequestCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("leave_request");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);

    const alix = await createTestClient();
    const bo = await createTestClient();

    const group = await alix
      .conversations()
      .createGroupByInboxIds([bo.inboxId]);

    await bo.conversations().sync();
    const boGroup = bo.conversations().findGroupById(group.id());
    await boGroup.leaveGroup();

    const messages = await boGroup.findMessages();
    const leaveRequest = LeaveRequestCodec.decode(messages[1].content);
    expect(leaveRequest).toMatchObject({});
  });

  test("should decode all content types", async () => {
    const alix = await createTestClient();
    const group = await alix.conversations().createGroupByInboxIds([]);
    const textMessage = await group.send(TextCodec.encode("Hello, world!"), {
      shouldPush: false,
    });
    await group.send(
      ReactionCodec.encode({
        reference: textMessage,
        referenceInboxId: "456",
        action: "added",
        content: "👍",
        schema: "unicode",
      }),
      { shouldPush: false }
    );
    await group.send(
      ReplyCodec.encode({
        content: TextCodec.encode("Hello, world!"),
        reference: textMessage,
        referenceInboxId: "456",
      }),
      { shouldPush: false }
    );
    await group.send(
      AttachmentCodec.encode({
        filename: "attachment.png",
        mimeType: "image/png",
        content: new Uint8Array([1, 2, 3, 4, 5]),
      }),
      { shouldPush: false }
    );
    await group.send(
      RemoteAttachmentCodec.encode({
        url: "https://example.com/attachment.png",
        contentDigest: "abc123",
        secret: new Uint8Array(32),
        salt: new Uint8Array(32),
        nonce: new Uint8Array(12),
        scheme: "https",
        contentLength: 100,
        filename: "attachment.png",
      }),
      { shouldPush: false }
    );
    await group.send(
      MultiRemoteAttachmentCodec.encode({
        attachments: [
          {
            secret: new Uint8Array([1, 2, 3]),
            contentDigest: "123",
            nonce: new Uint8Array([4, 5, 6]),
            scheme: "https",
            url: "https://example.com/attachment1.png",
            salt: new Uint8Array([7, 8, 9]),
            contentLength: 100,
            filename: "attachment1.png",
          },
          {
            secret: new Uint8Array([10, 11, 12]),
            contentDigest: "456",
            nonce: new Uint8Array([13, 14, 15]),
            scheme: "https",
            url: "https://example.com/attachment2.pdf",
            salt: new Uint8Array([16, 17, 18]),
          },
        ],
      }),
      { shouldPush: false }
    );
    await group.send(
      TransactionReferenceCodec.encode({
        namespace: "eip155",
        networkId: "1",
        reference: "0xabc123",
      }),
      { shouldPush: false }
    );
    await group.send(
      WalletSendCallsCodec.encode({
        version: "1",
        chainId: "1",
        from: "0x123",
        calls: [],
        capabilities: {
          foo: "bar",
        },
      }),
      { shouldPush: false }
    );
    await group.send(
      IntentCodec.encode({
        id: "1",
        actionId: "action-1",
        metadata: {
          foo: "bar",
        },
      }),
      { shouldPush: false }
    );
    await group.send(
      ActionsCodec.encode({
        id: "1",
        description: "Test actions",
        actions: [
          {
            id: "1",
            label: "Test action",
          },
        ],
      }),
      { shouldPush: false }
    );
    await group.send(ReadReceiptCodec.encode(), { shouldPush: false });

    const enrichedMessages = await group.findEnrichedMessages();
    // reactions and read receipts are not included
    expect(enrichedMessages.length).toBe(9);
    expect(enrichedMessages[0].content.type).toBe("text");
    expect(enrichedMessages[1].content.type).toBe("reply");
    expect(enrichedMessages[2].content.type).toBe("attachment");
    expect(enrichedMessages[3].content.type).toBe("remoteAttachment");
    expect(enrichedMessages[4].content.type).toBe("multiRemoteAttachment");
    expect(enrichedMessages[5].content.type).toBe("transactionReference");
    expect(enrichedMessages[6].content.type).toBe("walletSendCalls");
    expect(enrichedMessages[7].content.type).toBe("intent");
    expect(enrichedMessages[8].content.type).toBe("actions");

    expect(enrichedMessages[0].numReplies).toBe(1n);
    expect(enrichedMessages[0].reactions.length).toBe(1);
    expect(enrichedMessages[0].reactions[0].content.type).toBe("reaction");
    expect(enrichedMessages[0].reactions[0].content.content.action).toBe(
      "added"
    );
    expect(enrichedMessages[0].reactions[0].content.content.content).toBe("👍");
  });
});
