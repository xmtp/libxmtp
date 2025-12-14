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
  // Types
  Reaction,
  ReactionAction,
  ReactionSchema,
  Reply,
  ReadReceipt,
  Attachment,
  RemoteAttachment,
  RemoteAttachmentInfo,
  MultiRemoteAttachment,
  TransactionReference,
  TransactionMetadata,
  Actions,
  Action,
  ActionStyle,
  Intent,
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
    const reaction = new Reaction(
      "123",
      "456",
      ReactionAction.Added,
      "👍",
      ReactionSchema.Unicode
    );
    const encoded = ReactionCodec.encode(reaction);
    expect(encoded.fallback).toEqual(`Reacted with "👍" to an earlier message`);
    const decoded = ReactionCodec.decode(encoded);
    expect(decoded.reference).toEqual("123");
    expect(decoded.referenceInboxId).toEqual("456");
    expect(decoded.content).toEqual("👍");
    expect(ReactionCodec.shouldPush()).toBe(false);

    const reaction2 = new Reaction(
      "123",
      "456",
      ReactionAction.Removed,
      "👍",
      ReactionSchema.Unicode
    );
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
    const reply = new Reply(innerContent, "123", "456");
    const encoded = ReplyCodec.encode(reply);
    expect(encoded.fallback).toEqual(
      `Replied with "Hello, world!" to an earlier message`
    );
    const decoded = ReplyCodec.decode(encoded);
    expect(decoded.reference).toEqual("123");
    expect(decoded.referenceInboxId).toEqual("456");
    expect(ReplyCodec.shouldPush()).toBe(true);

    const attachmentContent = AttachmentCodec.encode(
      new Attachment(undefined, "image/png", new Uint8Array())
    );
    const reply2 = new Reply(attachmentContent, "123", "456");
    const encoded2 = ReplyCodec.encode(reply2);
    expect(encoded2.fallback).toEqual(`Replied to an earlier message`);
  });

  test("should encode and decode read receipts", () => {
    const contentType = ReadReceiptCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("readReceipt");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const readReceipt = new ReadReceipt();
    const encoded = ReadReceiptCodec.encode(readReceipt);
    expect(encoded.fallback).toEqual(undefined);
    const decoded = ReadReceiptCodec.decode(encoded);
    expect(decoded).toBeDefined();
    expect(ReadReceiptCodec.shouldPush()).toBe(false);
  });

  test("should encode and decode attachments", () => {
    const contentType = AttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("attachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const attachment = new Attachment(
      "attachment.png",
      "image/png",
      new Uint8Array([1, 2, 3, 4, 5])
    );
    const encoded = AttachmentCodec.encode(attachment);
    expect(encoded.fallback).toEqual(
      `Can't display attachment.png. This app doesn't support attachments.`
    );
    const decoded = AttachmentCodec.decode(encoded);
    expect(decoded.filename).toEqual("attachment.png");
    expect(decoded.mimeType).toEqual("image/png");
    expect(Array.from(decoded.content)).toEqual([1, 2, 3, 4, 5]);
    expect(AttachmentCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode remote attachments", () => {
    const contentType = RemoteAttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("remoteStaticAttachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const remoteAttachment = new RemoteAttachment(
      "https://example.com/attachment.png",
      "abc123",
      new Uint8Array(32),
      new Uint8Array(32),
      new Uint8Array(12),
      "https",
      100,
      "attachment.png"
    );
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
  });

  test("should encode and decode multi remote attachments", () => {
    const contentType = MultiRemoteAttachmentCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("multiRemoteStaticAttachment");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const attachments = [
      new RemoteAttachmentInfo(
        new Uint8Array(32),
        "123",
        new Uint8Array(12),
        "https",
        "https://example.com/attachment1.png",
        new Uint8Array(32),
        100,
        "attachment1.png"
      ),
      new RemoteAttachmentInfo(
        new Uint8Array(32),
        "456",
        new Uint8Array(12),
        "https",
        "https://example.com/attachment2.pdf",
        new Uint8Array(32),
        200,
        "attachment2.pdf"
      ),
    ];
    const multiRemoteAttachment = new MultiRemoteAttachment(attachments);
    const encoded = MultiRemoteAttachmentCodec.encode(multiRemoteAttachment);
    expect(encoded.fallback).toEqual(
      "Can't display this content. This app doesn't support multiple remote attachments."
    );
    const decoded = MultiRemoteAttachmentCodec.decode(encoded);
    expect(decoded.attachments.length).toEqual(2);
    expect(decoded.attachments[0].url).toEqual(
      "https://example.com/attachment1.png"
    );
    expect(decoded.attachments[1].url).toEqual(
      "https://example.com/attachment2.pdf"
    );
    expect(MultiRemoteAttachmentCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode transaction reference", () => {
    const contentType = TransactionReferenceCodec.contentType();
    expect(contentType.authorityId).toEqual("xmtp.org");
    expect(contentType.typeId).toEqual("transactionReference");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const transactionReference = new TransactionReference(
      "eip155",
      "1",
      "0xabc123",
      undefined
    );
    const encoded = TransactionReferenceCodec.encode(transactionReference);
    expect(encoded.fallback).toEqual(
      `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: 0xabc123`
    );
    const decoded = TransactionReferenceCodec.decode(encoded);
    expect(decoded.namespace).toEqual("eip155");
    expect(decoded.networkId).toEqual("1");
    expect(decoded.reference).toEqual("0xabc123");
    expect(TransactionReferenceCodec.shouldPush()).toBe(true);

    const transactionReference2 = new TransactionReference(
      "eip155",
      "1",
      "",
      undefined
    );
    const encoded2 = TransactionReferenceCodec.encode(transactionReference2);
    expect(encoded2.fallback).toEqual(`Crypto transaction`);
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
  });

  test("should encode and decode actions", () => {
    const contentType = ActionsCodec.contentType();
    expect(contentType.authorityId).toEqual("coinbase.com");
    expect(contentType.typeId).toEqual("actions");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const actions = new Actions("1", "Test actions", 1700000000000000000n);
    actions.addAction(
      new Action(
        "1",
        "Test action",
        "https://example.com/image.png",
        ActionStyle.Primary,
        1700000000000000000n
      )
    );
    actions.addAction(
      new Action(
        "2",
        "Test action 2",
        "https://example.com/image.png",
        ActionStyle.Secondary,
        1700000000000000000n
      )
    );
    const encoded = ActionsCodec.encode(actions);
    expect(encoded.fallback).toEqual(
      `Test actions\n\n[1] Test action\n[2] Test action 2\n\nReply with the number to select`
    );
    const decoded = ActionsCodec.decode(encoded);
    expect(decoded.id).toEqual("1");
    expect(decoded.description).toEqual("Test actions");
    const decodedActions = decoded.getActions();
    expect(decodedActions.length).toEqual(2);
    expect(decodedActions[0].label).toEqual("Test action");
    expect(decodedActions[1].label).toEqual("Test action 2");
    expect(ActionsCodec.shouldPush()).toBe(true);
  });

  test("should encode and decode intents", () => {
    const contentType = IntentCodec.contentType();
    expect(contentType.authorityId).toEqual("coinbase.com");
    expect(contentType.typeId).toEqual("intent");
    expect(contentType.versionMajor).toBeGreaterThan(0);
    expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
    const intent = new Intent("1", "action-1", { foo: "bar" });
    const encoded = IntentCodec.encode(intent);
    expect(encoded.fallback).toEqual(`User selected action: action-1`);
    const decoded = IntentCodec.decode(encoded);
    expect(decoded.id).toEqual("1");
    expect(decoded.actionId).toEqual("action-1");
    expect(IntentCodec.shouldPush()).toBe(true);
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
    expect(leaveRequest).toBeDefined();
  });
});
