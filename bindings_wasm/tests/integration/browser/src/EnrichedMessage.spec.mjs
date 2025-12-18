import { expect, test, describe } from "vitest";
import init, {
  // Encode functions (for reply content and testing)
  encodeText,
  encodeAttachment,
  encodeIntent,
  // Content type functions
  textContentType,
  markdownContentType,
  reactionContentType,
  replyContentType,
  readReceiptContentType,
  attachmentContentType,
  remoteAttachmentContentType,
  multiRemoteAttachmentContentType,
  transactionReferenceContentType,
  walletSendCallsContentType,
  actionsContentType,
  intentContentType,
  groupUpdatedContentType,
  leaveRequestContentType,
  // Test helpers
  createTestClient,
} from "@xmtp/wasm-bindings";

await init();

describe("EnrichedMessage", () => {
  // Helper to set up a basic group conversation
  const setupConversation = async () => {
    const client1 = await createTestClient();
    const client2 = await createTestClient();
    const conversation = await client1
      .conversations()
      .createGroupByInboxIds([client2.inboxId]);
    await client2.conversations().sync();
    const conversation2 = client2
      .conversations()
      .findGroupById(conversation.id());
    return { client1, client2, conversation, conversation2 };
  };

  describe("Basic message retrieval", () => {
    test("should return enriched messages with basic fields populated", async () => {
      const { client1, conversation } = await setupConversation();

      await conversation.sendText("Hello World");
      await conversation.sendText("Second message");

      const messages = await conversation.findEnrichedMessages();
      expect(messages.length).toEqual(3);

      const textMessages = messages.filter(
        (m) => m.content.type === "text" && m.content.content !== undefined
      );
      expect(textMessages.length).toEqual(2);

      const helloWorldMessage = textMessages.find(
        (m) => m.content.content === "Hello World"
      );
      expect(helloWorldMessage).toBeDefined();
      expect(helloWorldMessage.id).toBeDefined();
      expect(helloWorldMessage.sentAtNs).toBeDefined();
      expect(helloWorldMessage.senderInboxId).toBe(client1.inboxId);
      expect(helloWorldMessage.conversationId).toBeDefined();
      expect(helloWorldMessage.content.content).toBeDefined();
      expect(helloWorldMessage.content.content).toBe("Hello World");
      expect(helloWorldMessage.deliveryStatus).toBeDefined();
    });

    test("should handle list options", async () => {
      const { conversation } = await setupConversation();

      await conversation.sendText("Message 1");
      await conversation.sendText("Message 2");
      await conversation.sendText("Message 3");

      // Use plain object for tsify-based types
      const opts = {
        limit: 2n,
        direction: "descending",
      };
      const limitedMessages = await conversation.findEnrichedMessages(opts);
      const limitedTextMessages = limitedMessages.filter(
        (m) => m.content.type === "text"
      );
      expect(limitedTextMessages.length).toBe(2);

      const allMessages = await conversation.findEnrichedMessages();
      const allTextMessages = allMessages.filter(
        (m) => m.content.type === "text"
      );
      expect(allTextMessages.length).toEqual(3);
    });
  });

  describe("Message metadata", () => {
    test("should include message kind", async () => {
      const { conversation } = await setupConversation();

      await conversation.sendText("Test");

      const messages = await conversation.findEnrichedMessages();

      expect(messages.length).toEqual(2);
      // Messages should have kinds defined
      const messagesWithKind = messages.filter((m) => m.kind !== undefined);
      expect(messagesWithKind.length).toEqual(2);
    });
  });

  describe("Content types", () => {
    describe("Text", () => {
      test("should send and receive text message", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const messageId = await conversation.sendText("Hello, world!");
        expect(messageId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === messageId);
        expect(textMessage).toBeDefined();
        expect(textMessage.content.type).toBe("text");
        expect(textMessage.content.content).toBe("Hello, world!");
        expect(textMessage.senderInboxId).toBe(client1.inboxId);
        expect(textMessage.contentType?.authorityId).toBe("xmtp.org");
        expect(textMessage.contentType?.typeId).toBe("text");
        // Text has no fallback
        expect(textMessage.fallback).toBeUndefined();
      });

      test("should have correct content type", () => {
        const contentType = textContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("text");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Markdown", () => {
      test("should send and receive markdown messages", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const messageId = await conversation.sendMarkdown("# Hello, world!");
        expect(messageId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const markdownMessage = messages.find((m) => m.id === messageId);
        expect(markdownMessage).toBeDefined();
        expect(markdownMessage.content.type).toBe("markdown");
        expect(markdownMessage.content.content.content).toBe("# Hello, world!");
        expect(markdownMessage.senderInboxId).toBe(client1.inboxId);
        expect(markdownMessage.contentType?.authorityId).toBe("xmtp.org");
        expect(markdownMessage.contentType?.typeId).toBe("markdown");
        // Markdown has no fallback
        expect(markdownMessage.fallback).toBeUndefined();
      });

      test("should have correct content type", () => {
        const contentType = markdownContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("markdown");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Reaction", () => {
      test("should send and receive reaction with Added action", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: "added",
          content: "👍",
          schema: "unicode",
        });
        expect(reactionId).toBeDefined();

        await conversation2.sync();

        // Reactions are attached to parent messages in enriched messages
        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        expect(textMessage).toBeDefined();
        expect(textMessage.reactions.length).toBe(1);

        const reactionOnMessage = textMessage.reactions[0];
        expect(reactionOnMessage.id).toBe(reactionId);
        expect(reactionOnMessage.content.type).toBe("reaction");
        expect(reactionOnMessage.content.content?.content).toBe("👍");
        expect(reactionOnMessage.content.content?.action).toBe("added");
        expect(reactionOnMessage.content.content?.schema).toBe("unicode");
        expect(reactionOnMessage.senderInboxId).toBe(client1.inboxId);
        // Reaction Added fallback
        expect(reactionOnMessage.fallback).toBe(
          `Reacted with "👍" to an earlier message`
        );
      });

      test("should send and receive reaction with Removed action", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        // First add a reaction
        await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: "added",
          content: "👍",
          schema: "unicode",
        });

        // Then remove it
        const removeReactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: "removed",
          content: "👍",
          schema: "unicode",
        });
        expect(removeReactionId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        expect(textMessage).toBeDefined();
        // After removal, the reactions array should reflect the removal
        const removedReaction = textMessage.reactions.find(
          (r) => r.content.content?.action === "removed"
        );
        expect(removedReaction).toBeDefined();
        expect(removedReaction.content.content?.content).toBe("👍");
        // Reaction Removed fallback
        expect(removedReaction.fallback).toBe(
          `Removed "👍" from an earlier message`
        );
      });

      test("should handle shortcode reaction schema", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: "added",
          content: ":thumbsup:",
          schema: "shortcode",
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        const reaction = textMessage.reactions.find((r) => r.id === reactionId);
        expect(reaction).toBeDefined();
        expect(reaction.content.content?.content).toBe(":thumbsup:");
        expect(reaction.content.content?.schema).toBe("shortcode");
      });

      test("should handle custom reaction schema", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: "added",
          content: "custom-reaction-id",
          schema: "custom",
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        const reaction = textMessage.reactions.find((r) => r.id === reactionId);
        expect(reaction).toBeDefined();
        expect(reaction.content.content?.content).toBe("custom-reaction-id");
        expect(reaction.content.content?.schema).toBe("custom");
      });

      test("should have correct content type", () => {
        const contentType = reactionContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("reaction");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Reply", () => {
      test("should send and receive reply with text content", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Original message");

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          content: encodeText("This is a reply"),
        });
        expect(replyId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const replyMessage = messages.find((m) => m.id === replyId);
        expect(replyMessage).toBeDefined();
        expect(replyMessage.content.type).toBe("reply");
        expect(replyMessage.content.content?.referenceId).toBe(textMessageId);
        expect(replyMessage.content.content?.content.type).toBe("text");
        expect(replyMessage.content.content?.content.content).toBe(
          "This is a reply"
        );
        expect(replyMessage.senderInboxId).toBe(client1.inboxId);
        // Reply with text content fallback
        expect(replyMessage.fallback).toBe(
          `Replied with "This is a reply" to an earlier message`
        );
      });

      test("should include inReplyTo with original message", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const textMessageId = await conversation.sendText("Original message");

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          content: encodeText("Reply to original"),
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const replyMessage = messages.find((m) => m.id === replyId);
        expect(replyMessage).toBeDefined();
        expect(replyMessage.content.content?.inReplyTo).toBeDefined();
        expect(replyMessage.content.content?.inReplyTo?.content.content).toBe(
          "Original message"
        );
      });

      test("should send and receive reply with non-text content (attachment)", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const textMessageId = await conversation.sendText("Original message");

        const replyId = await conversation.sendReply({
          reference: textMessageId,
          content: encodeAttachment({
            filename: "reply.png",
            mimeType: "image/png",
            content: new Uint8Array([137, 80, 78, 71]),
          }),
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const replyMessage = messages.find((m) => m.id === replyId);
        expect(replyMessage).toBeDefined();
        expect(replyMessage.content.type).toBe("reply");
        expect(replyMessage.content.content?.content.type).toBe("attachment");
        // Reply with non-text content fallback (generic)
        expect(replyMessage.fallback).toBe(`Replied to an earlier message`);
      });

      test("should have correct content type", () => {
        const contentType = replyContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("reply");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Attachment", () => {
      test("should send and receive attachment", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const attachmentId = await conversation.sendAttachment({
          filename: "test.txt",
          mimeType: "text/plain",
          content: new Uint8Array([72, 101, 108, 108, 111]), // "Hello"
        });
        expect(attachmentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const attachmentMessage = messages.find((m) => m.id === attachmentId);
        expect(attachmentMessage).toBeDefined();
        expect(attachmentMessage.content.type).toBe("attachment");
        expect(attachmentMessage.content.content?.filename).toBe("test.txt");
        expect(attachmentMessage.content.content?.mimeType).toBe("text/plain");
        expect(attachmentMessage.content.content?.content).toEqual(
          new Uint8Array([72, 101, 108, 108, 111])
        );
        expect(attachmentMessage.contentType?.typeId).toBe("attachment");
        // Attachment fallback
        expect(attachmentMessage.fallback).toBe(
          `Can't display test.txt. This app doesn't support attachments.`
        );
      });

      test("should send and receive attachment without filename", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const attachmentId = await conversation.sendAttachment({
          mimeType: "image/png",
          content: new Uint8Array([137, 80, 78, 71]), // PNG header
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const attachmentMessage = messages.find((m) => m.id === attachmentId);
        expect(attachmentMessage).toBeDefined();
        expect(attachmentMessage.content.content?.filename).toBeUndefined();
        expect(attachmentMessage.content.content?.mimeType).toBe("image/png");
      });

      test("should have correct content type", () => {
        const contentType = attachmentContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("attachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Remote Attachment", () => {
      test("should send and receive remote attachment", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const remoteAttachmentId = await conversation.sendRemoteAttachment({
          url: "https://example.com/file.png",
          contentDigest: "abc123",
          secret: new Uint8Array([1, 2, 3]),
          salt: new Uint8Array([4, 5, 6]),
          nonce: new Uint8Array([7, 8, 9]),
          scheme: "https",
          contentLength: 1000,
          filename: "file.png",
        });
        expect(remoteAttachmentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const remoteAttachmentMessage = messages.find(
          (m) => m.id === remoteAttachmentId
        );
        expect(remoteAttachmentMessage).toBeDefined();
        expect(remoteAttachmentMessage.content.type).toBe("remoteAttachment");
        expect(remoteAttachmentMessage.content.content?.url).toBe(
          "https://example.com/file.png"
        );
        expect(remoteAttachmentMessage.content.content?.filename).toBe(
          "file.png"
        );
        expect(remoteAttachmentMessage.content.content?.contentDigest).toBe(
          "abc123"
        );
        expect(remoteAttachmentMessage.content.content?.scheme).toBe("https");
        expect(remoteAttachmentMessage.content.content?.contentLength).toBe(
          1000
        );
        expect(remoteAttachmentMessage.contentType?.typeId).toBe(
          "remoteStaticAttachment"
        );
        // Remote attachment fallback
        expect(remoteAttachmentMessage.fallback).toBe(
          `Can't display file.png. This app doesn't support remote attachments.`
        );
      });

      test("should send and receive remote attachment without filename", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const remoteAttachmentId = await conversation.sendRemoteAttachment({
          url: "https://example.com/file",
          contentDigest: "xyz789",
          secret: new Uint8Array([10, 11, 12]),
          salt: new Uint8Array([13, 14, 15]),
          nonce: new Uint8Array([16, 17, 18]),
          scheme: "https",
          contentLength: 500,
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const remoteAttachmentMessage = messages.find(
          (m) => m.id === remoteAttachmentId
        );
        expect(remoteAttachmentMessage).toBeDefined();
        expect(
          remoteAttachmentMessage.content.content?.filename
        ).toBeUndefined();
      });

      test("should have correct content type", () => {
        const contentType = remoteAttachmentContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("remoteStaticAttachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Multi Remote Attachment", () => {
      test("should send and receive multi remote attachment", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const multiRemoteAttachmentId =
          await conversation.sendMultiRemoteAttachment({
            attachments: [
              {
                url: "https://example.com/file1.png",
                contentDigest: "abc123",
                secret: new Uint8Array([1, 2, 3]),
                salt: new Uint8Array([4, 5, 6]),
                nonce: new Uint8Array([7, 8, 9]),
                scheme: "https",
                contentLength: 1000,
                filename: "file1.png",
              },
              {
                url: "https://example.com/file2.pdf",
                contentDigest: "def456",
                secret: new Uint8Array([10, 11, 12]),
                salt: new Uint8Array([13, 14, 15]),
                nonce: new Uint8Array([16, 17, 18]),
                scheme: "https",
                contentLength: 2000,
                filename: "file2.pdf",
              },
            ],
          });
        expect(multiRemoteAttachmentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const multiRemoteAttachmentMessage = messages.find(
          (m) => m.id === multiRemoteAttachmentId
        );
        expect(multiRemoteAttachmentMessage).toBeDefined();
        expect(multiRemoteAttachmentMessage.content.type).toBe(
          "multiRemoteAttachment"
        );
        expect(
          multiRemoteAttachmentMessage.content.content?.attachments.length
        ).toBe(2);
        expect(
          multiRemoteAttachmentMessage.content.content?.attachments[0].filename
        ).toBe("file1.png");
        expect(
          multiRemoteAttachmentMessage.content.content?.attachments[1].filename
        ).toBe("file2.pdf");
        expect(multiRemoteAttachmentMessage.contentType?.typeId).toBe(
          "multiRemoteStaticAttachment"
        );
        // Multi remote attachment fallback
        expect(multiRemoteAttachmentMessage.fallback).toBe(
          `Can't display this content. This app doesn't support multiple remote attachments.`
        );
      });

      test("should send and receive multi remote attachment with single attachment", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const multiRemoteAttachmentId =
          await conversation.sendMultiRemoteAttachment({
            attachments: [
              {
                url: "https://example.com/single.png",
                contentDigest: "single123",
                secret: new Uint8Array([1, 2, 3]),
                salt: new Uint8Array([4, 5, 6]),
                nonce: new Uint8Array([7, 8, 9]),
                scheme: "https",
                contentLength: 500,
                filename: "single.png",
              },
            ],
          });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const multiRemoteAttachmentMessage = messages.find(
          (m) => m.id === multiRemoteAttachmentId
        );
        expect(multiRemoteAttachmentMessage).toBeDefined();
        expect(
          multiRemoteAttachmentMessage.content.content?.attachments.length
        ).toBe(1);
      });

      test("should have correct content type", () => {
        const contentType = multiRemoteAttachmentContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("multiRemoteStaticAttachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Read Receipt", () => {
      test("should send read receipt (excluded from enriched messages by design)", async () => {
        const { conversation } = await setupConversation();

        const receiptId = await conversation.sendReadReceipt();
        expect(receiptId).toBeDefined();

        // Read receipts are excluded from enriched messages by design
        const messages = await conversation.findEnrichedMessages();
        const receiptMessage = messages.find((m) => m.id === receiptId);
        expect(receiptMessage).toBeUndefined();
      });

      test("should have correct content type", () => {
        const contentType = readReceiptContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("readReceipt");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Transaction Reference", () => {
      test("should send and receive transaction reference", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const transactionReferenceId =
          await conversation.sendTransactionReference({
            namespace: "eip155",
            networkId: "1",
            reference: "0x1234567890abcdef",
          });
        expect(transactionReferenceId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        );
        expect(transactionReferenceMessage).toBeDefined();
        expect(transactionReferenceMessage.content.type).toBe(
          "transactionReference"
        );
        expect(transactionReferenceMessage.content.content?.namespace).toBe(
          "eip155"
        );
        expect(transactionReferenceMessage.content.content?.networkId).toBe(
          "1"
        );
        expect(transactionReferenceMessage.content.content?.reference).toBe(
          "0x1234567890abcdef"
        );
        expect(transactionReferenceMessage.contentType?.typeId).toBe(
          "transactionReference"
        );
        // Transaction reference fallback with reference
        expect(transactionReferenceMessage.fallback).toBe(
          `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: 0x1234567890abcdef`
        );
      });

      test("should send and receive transaction reference without namespace", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const transactionReferenceId =
          await conversation.sendTransactionReference({
            networkId: "137",
            reference: "0xabcdef",
          });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        );
        expect(transactionReferenceMessage).toBeDefined();
        expect(
          transactionReferenceMessage.content.content?.namespace
        ).toBeUndefined();
        expect(transactionReferenceMessage.content.content?.networkId).toBe(
          "137"
        );
      });

      test("should send and receive transaction reference with empty reference", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const transactionReferenceId =
          await conversation.sendTransactionReference({
            namespace: "eip155",
            networkId: "1",
            reference: "",
          });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        );
        expect(transactionReferenceMessage).toBeDefined();
        // Transaction reference fallback without reference
        expect(transactionReferenceMessage.fallback).toBe(`Crypto transaction`);
      });

      test("should send and receive transaction reference with metadata", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const transactionReferenceId =
          await conversation.sendTransactionReference({
            namespace: "eip155",
            networkId: "1",
            reference: "0x123",
            metadata: {
              transactionType: "transfer",
              currency: "ETH",
              amount: 1.5,
              decimals: 18,
              fromAddress: "0xabc",
              toAddress: "0xdef",
            },
          });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId
        );
        expect(transactionReferenceMessage).toBeDefined();
        expect(
          transactionReferenceMessage.content.content?.metadata
        ).toBeDefined();
        expect(
          transactionReferenceMessage.content.content?.metadata?.transactionType
        ).toBe("transfer");
        expect(
          transactionReferenceMessage.content.content?.metadata?.currency
        ).toBe("ETH");
        expect(
          transactionReferenceMessage.content.content?.metadata?.amount
        ).toBe(1.5);
      });

      test("should have correct content type", () => {
        const contentType = transactionReferenceContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("transactionReference");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Wallet Send Calls", () => {
      test("should send and receive wallet send calls", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const walletSendCallsId = await conversation.sendWalletSendCalls({
          version: "1",
          chainId: "1",
          from: "0x1234567890abcdef1234567890abcdef12345678",
          calls: [
            {
              to: "0xabcdef1234567890abcdef1234567890abcdef12",
              data: "0x",
              value: "0x0",
            },
          ],
        });
        expect(walletSendCallsId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        );
        expect(walletSendCallsMessage).toBeDefined();
        expect(walletSendCallsMessage.content.type).toBe("walletSendCalls");
        expect(walletSendCallsMessage.content.content?.version).toBe("1");
        expect(walletSendCallsMessage.content.content?.chainId).toBe("1");
        expect(walletSendCallsMessage.content.content?.from).toBe(
          "0x1234567890abcdef1234567890abcdef12345678"
        );
        expect(walletSendCallsMessage.content.content?.calls.length).toBe(1);
        expect(walletSendCallsMessage.contentType?.typeId).toBe(
          "walletSendCalls"
        );
      });

      test("should send and receive wallet send calls with multiple calls", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const walletSendCallsId = await conversation.sendWalletSendCalls({
          version: "1",
          chainId: "137",
          from: "0x1234",
          calls: [
            {
              to: "0xabc",
              data: "0x123",
              value: "0x1",
            },
            {
              to: "0xdef",
              data: "0x456",
              value: "0x2",
              gas: "0x5208",
            },
          ],
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        );
        expect(walletSendCallsMessage).toBeDefined();
        expect(walletSendCallsMessage.content.content?.calls.length).toBe(2);
        expect(walletSendCallsMessage.content.content?.calls[0].to).toBe(
          "0xabc"
        );
        expect(walletSendCallsMessage.content.content?.calls[1].gas).toBe(
          "0x5208"
        );
      });

      test("should send and receive wallet send calls with metadata", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const walletSendCallsId = await conversation.sendWalletSendCalls({
          version: "1",
          chainId: "1",
          from: "0x1234",
          calls: [
            {
              to: "0xabc",
              data: "0x",
              value: "0x0",
              metadata: {
                description: "Send funds",
                transactionType: "transfer",
                note: "test payment",
              },
            },
          ],
          capabilities: {
            paymasterService: "https://paymaster.example.com",
          },
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const walletSendCallsMessage = messages.find(
          (m) => m.id === walletSendCallsId
        );
        expect(walletSendCallsMessage).toBeDefined();
        const metadata =
          walletSendCallsMessage.content.content?.calls[0].metadata;
        expect(metadata?.description).toBe("Send funds");
        expect(metadata?.transactionType).toBe("transfer");
        expect(metadata?.note).toBe("test payment");
        expect(
          walletSendCallsMessage.content.content?.capabilities?.paymasterService
        ).toBe("https://paymaster.example.com");
      });

      test("should error when metadata is missing `description` field", async () => {
        const { conversation } = await setupConversation();

        let error;
        try {
          await conversation.sendWalletSendCalls({
            version: "1",
            chainId: "1",
            from: "0x1234",
            calls: [
              {
                to: "0xabc",
                data: "0x",
                value: "0x0",
                metadata: {
                  transactionType: "transfer",
                },
              },
            ],
          });
        } catch (e) {
          error = e;
        }
        expect(error).toBeDefined();
        expect(error.message).toContain("missing field `description`");
      });

      test("should error when metadata is missing `transactionType` field", async () => {
        const { conversation } = await setupConversation();

        let error;
        try {
          await conversation.sendWalletSendCalls({
            version: "1",
            chainId: "1",
            from: "0x1234",
            calls: [
              {
                to: "0xabc",
                data: "0x",
                value: "0x0",
                metadata: {
                  description: "Send funds",
                },
              },
            ],
          });
        } catch (e) {
          error = e;
        }
        expect(error).toBeDefined();
        expect(error.message).toContain("missing field `transactionType`");
      });

      test("should have correct content type", () => {
        const contentType = walletSendCallsContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("walletSendCalls");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Actions", () => {
      test("should send and receive actions", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-1",
          description: "Choose an option",
          actions: [
            {
              id: "opt-1",
              label: "Option 1",
              style: "primary",
            },
            {
              id: "opt-2",
              label: "Option 2",
              style: "secondary",
            },
          ],
        });
        expect(actionsId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        expect(actionsMessage.content.type).toBe("actions");
        expect(actionsMessage.content.content?.id).toBe("action-1");
        expect(actionsMessage.content.content?.description).toBe(
          "Choose an option"
        );
        expect(actionsMessage.content.content?.actions.length).toBe(2);
        expect(actionsMessage.content.content?.actions[0].label).toBe(
          "Option 1"
        );
        expect(actionsMessage.content.content?.actions[0].style).toBe(
          "primary"
        );
        expect(actionsMessage.contentType?.authorityId).toBe("coinbase.com");
        expect(actionsMessage.contentType?.typeId).toBe("actions");
        // Actions fallback
        expect(actionsMessage.fallback).toBe(
          `Choose an option\n\n[1] Option 1\n[2] Option 2\n\nReply with the number to select`
        );
      });

      test("should send and receive actions with all styles", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-styles",
          description: "All styles",
          actions: [
            {
              id: "primary",
              label: "Primary",
              style: "primary",
            },
            {
              id: "secondary",
              label: "Secondary",
              style: "secondary",
            },
            {
              id: "danger",
              label: "Danger",
              style: "danger",
            },
          ],
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        expect(actionsMessage.content.content?.actions[0].style).toBe(
          "primary"
        );
        expect(actionsMessage.content.content?.actions[1].style).toBe(
          "secondary"
        );
        expect(actionsMessage.content.content?.actions[2].style).toBe("danger");
      });

      test("should send and receive actions with expiration", async () => {
        const { conversation, conversation2 } = await setupConversation();

        // Use a timestamp in nanoseconds (must fit in i64)
        const expiresAtNs = 1700000000000000000n; // Nov 2023 in nanoseconds

        const actionsId = await conversation.sendActions({
          id: "expiring-action",
          description: "Expiring action",
          actions: [
            {
              id: "opt-1",
              label: "Option 1",
              style: "primary",
              expiresAtNs: expiresAtNs,
            },
          ],
          expiresAtNs: expiresAtNs,
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        expect(actionsMessage.content.content?.expiresAtNs).toBeDefined();
        expect(
          actionsMessage.content.content?.actions[0].expiresAtNs
        ).toBeDefined();
      });

      test("should send and receive actions with image URL", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-with-image",
          description: "Action with image",
          actions: [
            {
              id: "opt-1",
              label: "Option 1",
              style: "primary",
              imageUrl: "https://example.com/image.png",
            },
          ],
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        expect(actionsMessage.content.content?.actions[0].imageUrl).toBe(
          "https://example.com/image.png"
        );
      });

      test("should have correct content type", () => {
        const contentType = actionsContentType();
        expect(contentType.authorityId).toEqual("coinbase.com");
        expect(contentType.typeId).toEqual("actions");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Intent", () => {
      test("should send and receive intent using encodeIntent", async () => {
        const { conversation, conversation2 } = await setupConversation();

        // Test with encodeIntent + send to verify the original pattern still works
        const intentId = await conversation.send(
          encodeIntent({
            id: "intent-1",
            actionId: "opt-1",
          }),
          { shouldPush: false }
        );
        expect(intentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const intentMessage = messages.find((m) => m.id === intentId);
        expect(intentMessage).toBeDefined();
        expect(intentMessage.content.type).toBe("intent");
        expect(intentMessage.content.content?.id).toBe("intent-1");
        expect(intentMessage.content.content?.actionId).toBe("opt-1");
        expect(intentMessage.contentType?.authorityId).toBe("coinbase.com");
        expect(intentMessage.contentType?.typeId).toBe("intent");
        // Intent fallback
        expect(intentMessage.fallback).toBe(`User selected action: opt-1`);
      });

      test("should send and receive intent using sendIntent", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const intentId = await conversation.sendIntent({
          id: "intent-1",
          actionId: "opt-1",
        });
        expect(intentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const intentMessage = messages.find((m) => m.id === intentId);
        expect(intentMessage).toBeDefined();
        expect(intentMessage.content.type).toBe("intent");
        expect(intentMessage.content.content?.id).toBe("intent-1");
        expect(intentMessage.content.content?.actionId).toBe("opt-1");
        expect(intentMessage.contentType?.authorityId).toBe("coinbase.com");
        expect(intentMessage.contentType?.typeId).toBe("intent");
        // Intent fallback
        expect(intentMessage.fallback).toBe(`User selected action: opt-1`);
      });

      test("should send and receive intent with metadata", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const intentId = await conversation.sendIntent({
          id: "intent-2",
          actionId: "opt-2",
          metadata: {
            source: "test",
            timestamp: "2024-01-01",
          },
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const intentMessage = messages.find((m) => m.id === intentId);
        expect(intentMessage).toBeDefined();
        expect(intentMessage.content.content?.metadata).toBeDefined();
        expect(intentMessage.content.content?.metadata?.source).toBe("test");
        expect(intentMessage.content.content?.metadata?.timestamp).toBe(
          "2024-01-01"
        );
      });

      test("should have correct content type", () => {
        const contentType = intentContentType();
        expect(contentType.authorityId).toEqual("coinbase.com");
        expect(contentType.typeId).toEqual("intent");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Group Updated", () => {
      test("should include group updated messages when members are added", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();
        const client3 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId]);

        await conversation.addMembersByInboxId([client3.inboxId]);

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated"
        );
        expect(groupUpdatedMessages.length).toBeGreaterThanOrEqual(2);

        const lastUpdate =
          groupUpdatedMessages[groupUpdatedMessages.length - 1];
        expect(lastUpdate.content.type).toBe("groupUpdated");
        expect(lastUpdate.content.content?.initiatedByInboxId).toBe(
          client1.inboxId
        );
        expect(lastUpdate.content.content?.addedInboxes.length).toBeGreaterThan(
          0
        );
      });

      test("should include group updated messages when members are removed", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();
        const client3 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId, client3.inboxId]);

        await conversation.removeMembersByInboxId([client2.inboxId]);

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated"
        );
        expect(groupUpdatedMessages.length).toBeGreaterThanOrEqual(2);

        const removalUpdate = groupUpdatedMessages.find(
          (m) =>
            m.content.content?.removedInboxes &&
            m.content.content.removedInboxes.length > 0
        );
        expect(removalUpdate).toBeDefined();
        expect(removalUpdate.content.content?.removedInboxes[0].inboxId).toBe(
          client2.inboxId
        );
      });

      test("should include group updated messages when metadata is changed", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId]);

        await conversation.updateGroupName("New Group Name");

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated"
        );

        const metadataUpdate = groupUpdatedMessages.find(
          (m) =>
            m.content.content?.metadataFieldChanges &&
            m.content.content.metadataFieldChanges.length > 0
        );
        expect(metadataUpdate).toBeDefined();
        expect(
          metadataUpdate.content.content?.metadataFieldChanges[0].newValue
        ).toBe("New Group Name");
      });

      test("should have correct content type", () => {
        const contentType = groupUpdatedContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("group_updated");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Leave Request", () => {
      test("should have correct content type", () => {
        const contentType = leaveRequestContentType();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("leave_request");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });
  });
});
