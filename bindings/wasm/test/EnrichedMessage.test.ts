import { describe, expect, it } from "vitest";
import init, {
  Actions,
  ActionStyle,
  Attachment,
  contentTypeActions,
  contentTypeAttachment,
  contentTypeGroupUpdated,
  contentTypeIntent,
  contentTypeLeaveRequest,
  contentTypeMarkdown,
  contentTypeMultiRemoteAttachment,
  contentTypeReaction,
  contentTypeReadReceipt,
  contentTypeRemoteAttachment,
  contentTypeReply,
  // Content type functions
  contentTypeText,
  contentTypeTransactionReference,
  contentTypeWalletSendCalls,
  // Test helpers
  createTestClient,
  encodeAttachment,
  encodeIntent,
  // Encode functions (for reply content and testing)
  encodeText,
  EnrichedReply,
  GroupUpdated,
  Intent,
  MultiRemoteAttachment,
  Reaction,
  ReactionAction,
  ReactionSchema,
  RemoteAttachment,
  SortDirection,
  TransactionReference,
  WalletSendCalls,
} from "../";

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
    it("should return enriched messages with basic fields populated", async () => {
      const { client1, conversation } = await setupConversation();

      await conversation.sendText("Hello World");
      await conversation.sendText("Second message");

      const messages = await conversation.findEnrichedMessages();
      expect(messages.length).toEqual(3);

      const textMessages = messages.filter(
        (m) => m.content.type === "text" && m.content.content !== undefined,
      );
      expect(textMessages.length).toEqual(2);

      const helloWorldMessage = textMessages.find(
        (m) => m.content.content === "Hello World",
      );
      expect(helloWorldMessage).toBeDefined();
      expect(helloWorldMessage?.id).toBeDefined();
      expect(helloWorldMessage?.sentAtNs).toBeDefined();
      expect(helloWorldMessage?.senderInboxId).toBe(client1.inboxId);
      expect(helloWorldMessage?.conversationId).toBeDefined();
      expect(helloWorldMessage?.content.content).toBeDefined();
      expect(helloWorldMessage?.content.content).toBe("Hello World");
      expect(helloWorldMessage?.deliveryStatus).toBeDefined();
    });

    it("should handle list options", async () => {
      const { conversation } = await setupConversation();

      await conversation.sendText("Message 1");
      await conversation.sendText("Message 2");
      await conversation.sendText("Message 3");

      // Use plain object for tsify-based types
      const opts = {
        limit: 2n,
        direction: SortDirection.Descending,
      };
      const limitedMessages = await conversation.findEnrichedMessages(opts);
      const limitedTextMessages = limitedMessages.filter(
        (m) => m.content.type === "text",
      );
      expect(limitedTextMessages.length).toBe(2);

      const allMessages = await conversation.findEnrichedMessages();
      const allTextMessages = allMessages.filter(
        (m) => m.content.type === "text",
      );
      expect(allTextMessages.length).toEqual(3);
    });
  });

  describe("Message metadata", () => {
    it("should include message kind", async () => {
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
      it("should send and receive text message", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const messageId = await conversation.sendText("Hello, world!");
        expect(messageId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === messageId);
        expect(textMessage).toBeDefined();
        expect(textMessage?.content.type).toBe("text");
        expect(textMessage?.content.content).toBe("Hello, world!");
        expect(textMessage?.senderInboxId).toBe(client1.inboxId);
        expect(textMessage?.contentType?.authorityId).toBe("xmtp.org");
        expect(textMessage?.contentType?.typeId).toBe("text");
        // Text has no fallback
        expect(textMessage?.fallback).toBeUndefined();
      });

      it("should have correct content type", () => {
        const contentType = contentTypeText();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("text");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Markdown", () => {
      it("should send and receive markdown messages", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const messageId = await conversation.sendMarkdown("# Hello, world!");
        expect(messageId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const markdownMessage = messages.find((m) => m.id === messageId);
        expect(markdownMessage).toBeDefined();
        expect(markdownMessage?.content.type).toBe("markdown");
        expect(markdownMessage?.content.content).toBe("# Hello, world!");
        expect(markdownMessage?.senderInboxId).toBe(client1.inboxId);
        expect(markdownMessage?.contentType?.authorityId).toBe("xmtp.org");
        expect(markdownMessage?.contentType?.typeId).toBe("markdown");
        // Markdown has no fallback
        expect(markdownMessage?.fallback).toBeUndefined();
      });

      it("should have correct content type", () => {
        const contentType = contentTypeMarkdown();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("markdown");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Reaction", () => {
      it("should send and receive reaction with Added action", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: ReactionAction.Added,
          content: "👍",
          schema: ReactionSchema.Unicode,
        });
        expect(reactionId).toBeDefined();

        await conversation2.sync();

        // Reactions are attached to parent messages in enriched messages
        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        expect(textMessage).toBeDefined();
        expect(textMessage?.reactions.length).toBe(1);

        const reactionOnMessage = textMessage?.reactions[0];
        expect(reactionOnMessage?.id).toBe(reactionId);
        expect(reactionOnMessage?.content.type).toBe("reaction");
        const reactionContent = reactionOnMessage?.content.content as Reaction;
        expect(reactionContent.content).toBe("👍");
        expect(reactionContent.action).toBe(ReactionAction.Added);
        expect(reactionContent.schema).toBe(ReactionSchema.Unicode);
        expect(reactionOnMessage?.senderInboxId).toBe(client1.inboxId);
        // Reaction Added fallback
        expect(reactionOnMessage?.fallback).toBe(
          `Reacted with "👍" to an earlier message`,
        );
      });

      it("should send and receive reaction with Removed action", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        // First add a reaction
        await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: ReactionAction.Added,
          content: "👍",
          schema: ReactionSchema.Unicode,
        });

        // Then remove it
        const removeReactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: ReactionAction.Removed,
          content: "👍",
          schema: ReactionSchema.Unicode,
        });
        expect(removeReactionId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        expect(textMessage).toBeDefined();
        // After removal, the reactions array should reflect the removal
        const removedReaction = textMessage?.reactions.find(
          (r) =>
            (r.content.content as Reaction).action === ReactionAction.Removed,
        );
        expect(removedReaction).toBeDefined();
        const removedReactionContent = removedReaction?.content
          .content as Reaction;
        expect(removedReactionContent.content).toBe("👍");
        // Reaction Removed fallback
        expect(removedReaction?.fallback).toBe(
          `Removed "👍" from an earlier message`,
        );
      });

      it("should handle shortcode reaction schema", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: ReactionAction.Added,
          content: ":thumbsup:",
          schema: ReactionSchema.Shortcode,
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        const reaction = textMessage?.reactions.find(
          (r) => r.id === reactionId,
        );
        expect(reaction).toBeDefined();
        const reactionContent = reaction?.content.content as Reaction;
        expect(reactionContent.content).toBe(":thumbsup:");
        expect(reactionContent.schema).toBe(ReactionSchema.Shortcode);
      });

      it("should handle custom reaction schema", async () => {
        const { client1, conversation, conversation2 } =
          await setupConversation();

        const textMessageId = await conversation.sendText("Hello!");

        const reactionId = await conversation.sendReaction({
          reference: textMessageId,
          referenceInboxId: client1.inboxId,
          action: ReactionAction.Added,
          content: "custom-reaction-id",
          schema: ReactionSchema.Custom,
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const textMessage = messages.find((m) => m.id === textMessageId);
        const reaction = textMessage?.reactions.find(
          (r) => r.id === reactionId,
        );
        expect(reaction).toBeDefined();
        const reactionContent = reaction?.content.content as Reaction;
        expect(reactionContent.content).toBe("custom-reaction-id");
        expect(reactionContent.schema).toBe(ReactionSchema.Custom);
      });

      it("should have correct content type", () => {
        const contentType = contentTypeReaction();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("reaction");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Reply", () => {
      it("should send and receive reply with text content", async () => {
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
        expect(replyMessage?.content.type).toBe("reply");
        const replyContent = replyMessage?.content.content as EnrichedReply;
        expect(replyContent.referenceId).toBe(textMessageId);
        expect(replyContent.content.type).toBe("text");
        expect(replyContent.content.content).toBe("This is a reply");
        expect(replyMessage?.senderInboxId).toBe(client1.inboxId);
        // Reply with text content fallback
        expect(replyMessage?.fallback).toBe(
          `Replied with "This is a reply" to an earlier message`,
        );
      });

      it("should include inReplyTo with original message", async () => {
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
        const replyContent = replyMessage?.content.content as EnrichedReply;
        expect(replyContent.inReplyTo).toBeDefined();
        expect(replyContent.inReplyTo?.content.content).toBe(
          "Original message",
        );
      });

      it("should send and receive reply with non-text content (attachment)", async () => {
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
        expect(replyMessage?.content.type).toBe("reply");
        const replyContent = replyMessage?.content.content as EnrichedReply;
        expect(replyContent.content.type).toBe("attachment");
        // Reply with non-text content fallback (generic)
        expect(replyMessage?.fallback).toBe(`Replied to an earlier message`);
      });

      it("should have correct content type", () => {
        const contentType = contentTypeReply();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("reply");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Attachment", () => {
      it("should send and receive attachment", async () => {
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
        expect(attachmentMessage?.content.type).toBe("attachment");
        const attachmentContent = attachmentMessage?.content
          .content as Attachment;
        expect(attachmentContent.filename).toBe("test.txt");
        expect(attachmentContent.mimeType).toBe("text/plain");
        expect(attachmentContent.content).toEqual(
          new Uint8Array([72, 101, 108, 108, 111]),
        );
        expect(attachmentMessage?.contentType?.typeId).toBe("attachment");
        // Attachment fallback
        expect(attachmentMessage?.fallback).toBe(
          `Can't display test.txt. This app doesn't support attachments.`,
        );
      });

      it("should send and receive attachment without filename", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const attachmentId = await conversation.sendAttachment({
          mimeType: "image/png",
          content: new Uint8Array([137, 80, 78, 71]), // PNG header
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const attachmentMessage = messages.find((m) => m.id === attachmentId);
        expect(attachmentMessage).toBeDefined();
        const attachmentContent = attachmentMessage?.content
          .content as Attachment;
        expect(attachmentContent.filename).toBeUndefined();
        expect(attachmentContent.mimeType).toBe("image/png");
      });

      it("should have correct content type", () => {
        const contentType = contentTypeAttachment();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("attachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Remote Attachment", () => {
      it("should send and receive remote attachment", async () => {
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
          (m) => m.id === remoteAttachmentId,
        );
        expect(remoteAttachmentMessage).toBeDefined();
        expect(remoteAttachmentMessage?.content.type).toBe("remoteAttachment");
        const remoteAttachmentContent = remoteAttachmentMessage?.content
          .content as RemoteAttachment;
        expect(remoteAttachmentContent.url).toBe(
          "https://example.com/file.png",
        );
        expect(remoteAttachmentContent.filename).toBe("file.png");
        expect(remoteAttachmentContent.contentDigest).toBe("abc123");
        expect(remoteAttachmentContent.scheme).toBe("https");
        expect(remoteAttachmentContent.contentLength).toBe(1000);
        expect(remoteAttachmentMessage?.contentType?.typeId).toBe(
          "remoteStaticAttachment",
        );
        // Remote attachment fallback
        expect(remoteAttachmentMessage?.fallback).toBe(
          `Can't display file.png. This app doesn't support remote attachments.`,
        );
      });

      it("should send and receive remote attachment without filename", async () => {
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
          (m) => m.id === remoteAttachmentId,
        );
        expect(remoteAttachmentMessage).toBeDefined();
        const remoteAttachmentContent = remoteAttachmentMessage?.content
          .content as RemoteAttachment;
        expect(remoteAttachmentContent.filename).toBeUndefined();
      });

      it("should have correct content type", () => {
        const contentType = contentTypeRemoteAttachment();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("remoteStaticAttachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Multi Remote Attachment", () => {
      it("should send and receive multi remote attachment", async () => {
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
          (m) => m.id === multiRemoteAttachmentId,
        );
        expect(multiRemoteAttachmentMessage).toBeDefined();
        expect(multiRemoteAttachmentMessage?.content.type).toBe(
          "multiRemoteAttachment",
        );
        const multiRemoteAttachmentContent = multiRemoteAttachmentMessage
          ?.content.content as MultiRemoteAttachment;
        expect(multiRemoteAttachmentContent.attachments.length).toBe(2);
        expect(multiRemoteAttachmentContent.attachments[0].filename).toBe(
          "file1.png",
        );
        expect(multiRemoteAttachmentContent.attachments[1].filename).toBe(
          "file2.pdf",
        );
        expect(multiRemoteAttachmentMessage?.contentType?.typeId).toBe(
          "multiRemoteStaticAttachment",
        );
        // Multi remote attachment fallback
        expect(multiRemoteAttachmentMessage?.fallback).toBe(
          `Can't display this content. This app doesn't support multiple remote attachments.`,
        );
      });

      it("should send and receive multi remote attachment with single attachment", async () => {
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
          (m) => m.id === multiRemoteAttachmentId,
        );
        expect(multiRemoteAttachmentMessage).toBeDefined();
        const multiRemoteAttachmentContent = multiRemoteAttachmentMessage
          ?.content.content as MultiRemoteAttachment;
        expect(multiRemoteAttachmentContent.attachments.length).toBe(1);
      });

      it("should have correct content type", () => {
        const contentType = contentTypeMultiRemoteAttachment();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("multiRemoteStaticAttachment");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Read Receipt", () => {
      it("should send read receipt (excluded from enriched messages by design)", async () => {
        const { conversation } = await setupConversation();

        const receiptId = await conversation.sendReadReceipt();
        expect(receiptId).toBeDefined();

        // Read receipts are excluded from enriched messages by design
        const messages = await conversation.findEnrichedMessages();
        const receiptMessage = messages.find((m) => m.id === receiptId);
        expect(receiptMessage).toBeUndefined();
      });

      it("should have correct content type", () => {
        const contentType = contentTypeReadReceipt();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("readReceipt");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Transaction Reference", () => {
      it("should send and receive transaction reference", async () => {
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
          (m) => m.id === transactionReferenceId,
        );
        expect(transactionReferenceMessage).toBeDefined();
        expect(transactionReferenceMessage?.content.type).toBe(
          "transactionReference",
        );
        const transactionReferenceContent = transactionReferenceMessage?.content
          .content as TransactionReference;
        expect(transactionReferenceContent.namespace).toBe("eip155");
        expect(transactionReferenceContent.networkId).toBe("1");
        expect(transactionReferenceContent.reference).toBe(
          "0x1234567890abcdef",
        );
        expect(transactionReferenceMessage?.contentType?.typeId).toBe(
          "transactionReference",
        );
        // Transaction reference fallback with reference
        expect(transactionReferenceMessage?.fallback).toBe(
          `[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: 0x1234567890abcdef`,
        );
      });

      it("should send and receive transaction reference without namespace", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const transactionReferenceId =
          await conversation.sendTransactionReference({
            networkId: "137",
            reference: "0xabcdef",
          });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const transactionReferenceMessage = messages.find(
          (m) => m.id === transactionReferenceId,
        );
        expect(transactionReferenceMessage).toBeDefined();
        const transactionReferenceContent = transactionReferenceMessage?.content
          .content as TransactionReference;
        expect(transactionReferenceContent.namespace).toBeUndefined();
        expect(transactionReferenceContent.networkId).toBe("137");
      });

      it("should send and receive transaction reference with empty reference", async () => {
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
          (m) => m.id === transactionReferenceId,
        );
        expect(transactionReferenceMessage).toBeDefined();
        // Transaction reference fallback without reference
        expect(transactionReferenceMessage?.fallback).toBe(
          `Crypto transaction`,
        );
      });

      it("should send and receive transaction reference with metadata", async () => {
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
          (m) => m.id === transactionReferenceId,
        );
        expect(transactionReferenceMessage).toBeDefined();
        const transactionReferenceContent = transactionReferenceMessage?.content
          .content as TransactionReference;
        expect(transactionReferenceContent.metadata).toBeDefined();
        expect(transactionReferenceContent.metadata?.transactionType).toBe(
          "transfer",
        );
        expect(transactionReferenceContent.metadata?.currency).toBe("ETH");
        expect(transactionReferenceContent.metadata?.amount).toBe(1.5);
      });

      it("should have correct content type", () => {
        const contentType = contentTypeTransactionReference();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("transactionReference");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Wallet Send Calls", () => {
      it("should send and receive wallet send calls", async () => {
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
          (m) => m.id === walletSendCallsId,
        );
        expect(walletSendCallsMessage).toBeDefined();
        expect(walletSendCallsMessage?.content.type).toBe("walletSendCalls");
        const walletSendCallsContent = walletSendCallsMessage?.content
          .content as WalletSendCalls;
        expect(walletSendCallsContent.version).toBe("1");
        expect(walletSendCallsContent.chainId).toBe("1");
        expect(walletSendCallsContent.from).toBe(
          "0x1234567890abcdef1234567890abcdef12345678",
        );
        expect(walletSendCallsContent.calls.length).toBe(1);
        expect(walletSendCallsMessage?.contentType?.typeId).toBe(
          "walletSendCalls",
        );
        expect(walletSendCallsContent.chainId).toBe("1");
        expect(walletSendCallsContent.from).toBe(
          "0x1234567890abcdef1234567890abcdef12345678",
        );
        expect(walletSendCallsContent.calls.length).toBe(1);
        expect(walletSendCallsMessage?.contentType?.typeId).toBe(
          "walletSendCalls",
        );
      });

      it("should send and receive wallet send calls with multiple calls", async () => {
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
          (m) => m.id === walletSendCallsId,
        );
        expect(walletSendCallsMessage).toBeDefined();
        const walletSendCallsContent = walletSendCallsMessage?.content
          .content as WalletSendCalls;
        expect(walletSendCallsContent.calls.length).toBe(2);
        expect(walletSendCallsContent.calls[0].to).toBe("0xabc");
        expect(walletSendCallsContent.calls[1].gas).toBe("0x5208");
      });

      it("should send and receive wallet send calls with metadata", async () => {
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
          (m) => m.id === walletSendCallsId,
        );
        expect(walletSendCallsMessage).toBeDefined();
        const walletSendCallsContent = walletSendCallsMessage?.content
          .content as WalletSendCalls;
        const metadata = walletSendCallsContent.calls[0].metadata;
        expect(metadata?.description).toBe("Send funds");
        expect(metadata?.transactionType).toBe("transfer");
        expect(metadata?.note).toBe("test payment");
        expect(walletSendCallsContent.capabilities?.paymasterService).toBe(
          "https://paymaster.example.com",
        );
      });

      it("should error when metadata is missing `description` field", async () => {
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
        expect((error as Error).message).toBe("missing field `description`");
      });

      it("should error when metadata is missing `transactionType` field", async () => {
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
        expect((error as Error).message).toBe(
          "missing field `transactionType`",
        );
      });

      it("should have correct content type", () => {
        const contentType = contentTypeWalletSendCalls();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("walletSendCalls");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Actions", () => {
      it("should send and receive actions", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-1",
          description: "Choose an option",
          actions: [
            {
              id: "opt-1",
              label: "Option 1",
              style: ActionStyle.Primary,
            },
            {
              id: "opt-2",
              label: "Option 2",
              style: ActionStyle.Secondary,
            },
          ],
        });
        expect(actionsId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        expect(actionsMessage?.content.type).toBe("actions");
        const actionsContent = actionsMessage?.content.content as Actions;
        expect(actionsContent.id).toBe("action-1");
        expect(actionsContent.description).toBe("Choose an option");
        expect(actionsContent.actions.length).toBe(2);
        expect(actionsContent.actions[0].label).toBe("Option 1");
        expect(actionsContent.actions[0].style).toBe(ActionStyle.Primary);
        expect(actionsMessage?.contentType?.authorityId).toBe("coinbase.com");
        expect(actionsMessage?.contentType?.typeId).toBe("actions");
        // Actions fallback
        expect(actionsMessage?.fallback).toBe(
          `Choose an option\n\n[1] Option 1\n[2] Option 2\n\nReply with the number to select`,
        );
      });

      it("should send and receive actions with all styles", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-styles",
          description: "All styles",
          actions: [
            {
              id: "primary",
              label: "Primary",
              style: ActionStyle.Primary,
            },
            {
              id: "secondary",
              label: "Secondary",
              style: ActionStyle.Secondary,
            },
            {
              id: "danger",
              label: "Danger",
              style: ActionStyle.Danger,
            },
          ],
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        const actionsContent = actionsMessage?.content.content as Actions;
        expect(actionsContent.actions[0].style).toBe(ActionStyle.Primary);
        expect(actionsContent.actions[1].style).toBe(ActionStyle.Secondary);
        expect(actionsContent.actions[2].style).toBe(ActionStyle.Danger);
      });

      it("should send and receive actions with expiration", async () => {
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
              style: ActionStyle.Primary,
              expiresAtNs: expiresAtNs,
            },
          ],
          expiresAtNs: expiresAtNs,
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        const actionsContent = actionsMessage?.content.content as Actions;
        expect(actionsContent.expiresAtNs).toBeDefined();
        expect(actionsContent.actions[0].expiresAtNs).toBeDefined();
      });

      it("should send and receive actions with image URL", async () => {
        const { conversation, conversation2 } = await setupConversation();

        const actionsId = await conversation.sendActions({
          id: "action-with-image",
          description: "Action with image",
          actions: [
            {
              id: "opt-1",
              label: "Option 1",
              style: ActionStyle.Primary,
              imageUrl: "https://example.com/image.png",
            },
          ],
        });

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const actionsMessage = messages.find((m) => m.id === actionsId);
        expect(actionsMessage).toBeDefined();
        const actionsContent = actionsMessage?.content.content as Actions;
        expect(actionsContent.actions[0].imageUrl).toBe(
          "https://example.com/image.png",
        );
      });

      it("should have correct content type", () => {
        const contentType = contentTypeActions();
        expect(contentType.authorityId).toEqual("coinbase.com");
        expect(contentType.typeId).toEqual("actions");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Intent", () => {
      it("should send and receive intent using encodeIntent", async () => {
        const { conversation, conversation2 } = await setupConversation();

        // Test with encodeIntent + send to verify the original pattern still works
        const intentId = await conversation.send(
          encodeIntent({
            id: "intent-1",
            actionId: "opt-1",
          }),
          { shouldPush: false },
        );
        expect(intentId).toBeDefined();

        await conversation2.sync();

        const messages = await conversation2.findEnrichedMessages();
        const intentMessage = messages.find((m) => m.id === intentId);
        expect(intentMessage).toBeDefined();
        expect(intentMessage?.content.type).toBe("intent");
        const intentContent = intentMessage?.content.content as Intent;
        expect(intentContent.id).toBe("intent-1");
        expect(intentContent.actionId).toBe("opt-1");
        expect(intentMessage?.contentType?.authorityId).toBe("coinbase.com");
        expect(intentMessage?.contentType?.typeId).toBe("intent");
        // Intent fallback
        expect(intentMessage?.fallback).toBe(`User selected action: opt-1`);
      });

      it("should send and receive intent using sendIntent", async () => {
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
        expect(intentMessage?.content.type).toBe("intent");
        const intentContent = intentMessage?.content.content as Intent;
        expect(intentContent.id).toBe("intent-1");
        expect(intentContent.actionId).toBe("opt-1");
        expect(intentMessage?.contentType?.authorityId).toBe("coinbase.com");
        expect(intentMessage?.contentType?.typeId).toBe("intent");
        // Intent fallback
        expect(intentMessage?.fallback).toBe(`User selected action: opt-1`);
      });

      it("should send and receive intent with metadata", async () => {
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
        const intentContent = intentMessage?.content.content as Intent;
        expect(intentContent.metadata).toBeDefined();
        expect(intentContent.metadata?.source).toBe("test");
        expect(intentContent.metadata?.timestamp).toBe("2024-01-01");
      });

      it("should have correct content type", () => {
        const contentType = contentTypeIntent();
        expect(contentType.authorityId).toEqual("coinbase.com");
        expect(contentType.typeId).toEqual("intent");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Group Updated", () => {
      it("should include group updated messages when members are added", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();
        const client3 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId]);

        await conversation.addMembers([client3.inboxId]);

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated",
        );
        expect(groupUpdatedMessages.length).toBeGreaterThanOrEqual(2);

        const lastUpdate =
          groupUpdatedMessages[groupUpdatedMessages.length - 1];
        expect(lastUpdate.content.type).toBe("groupUpdated");
        const groupUpdatedContent = lastUpdate.content.content as GroupUpdated;
        expect(groupUpdatedContent.initiatedByInboxId).toBe(client1.inboxId);
        expect(groupUpdatedContent.addedInboxes.length).toBeGreaterThan(0);
      });

      it("should include group updated messages when members are removed", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();
        const client3 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId, client3.inboxId]);

        await conversation.removeMembers([client2.inboxId]);

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated",
        );
        expect(groupUpdatedMessages.length).toBeGreaterThanOrEqual(2);

        const groupUpdatedContent = groupUpdatedMessages[
          groupUpdatedMessages.length - 1
        ].content.content as GroupUpdated;
        expect(groupUpdatedContent.removedInboxes.length).toBeGreaterThan(0);
        expect(groupUpdatedContent.removedInboxes[0].inboxId).toBe(
          client2.inboxId,
        );
      });

      it("should include group updated messages when metadata is changed", async () => {
        const client1 = await createTestClient();
        const client2 = await createTestClient();

        const conversation = await client1
          .conversations()
          .createGroupByInboxIds([client2.inboxId]);

        await conversation.updateGroupName("New Group Name");

        const messages = await conversation.findEnrichedMessages();
        const groupUpdatedMessages = messages.filter(
          (m) => m.content.type === "groupUpdated",
        );
        const groupUpdatedContent = groupUpdatedMessages[
          groupUpdatedMessages.length - 1
        ].content.content as GroupUpdated;
        expect(groupUpdatedContent.metadataFieldChanges[0].newValue).toBe(
          "New Group Name",
        );
      });

      it("should have correct content type", () => {
        const contentType = contentTypeGroupUpdated();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("group_updated");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });

    describe("Leave Request", () => {
      it("should have correct content type", () => {
        const contentType = contentTypeLeaveRequest();
        expect(contentType.authorityId).toEqual("xmtp.org");
        expect(contentType.typeId).toEqual("leave_request");
        expect(contentType.versionMajor).toBeGreaterThan(0);
        expect(contentType.versionMinor).toBeGreaterThanOrEqual(0);
      });
    });
  });
});
