import {
  encodeText,
  type DecodedMessage,
  type EnrichedReply,
} from "@xmtp/node-sdk";
import { describe, expect, expectTypeOf, it, vi } from "vitest";
import type { Client, Conversation } from "@xmtp/node-sdk";
import type { DecodedMessageWithContent } from "@/core/filter";
import { MessageContext } from "@/core/MessageContext";
import { createClient } from "@/util/test";

describe("MessageContext", () => {
  it("should properly type the content when using reply as input", async () => {
    const client = await createClient();
    const group = await client.conversations.createGroup([]);
    const messageId = await group.sendReply({
      reference: "message-id",
      referenceInboxId: "sender-inbox-id",
      content: encodeText("This is a reply"),
    });
    const replyMessage = client.conversations.getMessageById(
      messageId,
    )! as DecodedMessage<EnrichedReply<string>>;
    const messageContext = new MessageContext({
      message: replyMessage,
      conversation: group,
      client,
    });

    const typedContext = messageContext as MessageContext<
      EnrichedReply<string>
    >;
    expectTypeOf(typedContext.message.content).toEqualTypeOf<
      EnrichedReply<string>
    >();
    const { content } = typedContext.message;
    expect(content.content).toBe(replyMessage.content?.content);
  });

  it("should send read receipt via sendReadReceipt and markAsRead", async () => {
    const sendReadReceiptMock = vi.fn().mockResolvedValue("receipt-id-123");
    const mockConversation = {
      sendReadReceipt: sendReadReceiptMock,
    } as unknown as Conversation;

    const messageContext = new MessageContext({
      message: {} as DecodedMessageWithContent<string>,
      conversation: mockConversation,
      client: {} as Client,
    });

    const result1 = await messageContext.sendReadReceipt();
    expect(result1).toBe("receipt-id-123");
    expect(sendReadReceiptMock).toHaveBeenCalledTimes(1);

    const result2 = await messageContext.markAsRead();
    expect(result2).toBe("receipt-id-123");
    expect(sendReadReceiptMock).toHaveBeenCalledTimes(2);
  });
});
