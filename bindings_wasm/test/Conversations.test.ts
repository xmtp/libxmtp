import { describe, expect, it } from "vitest";
import init, { createTestClient, DecodedMessage } from "../";

await init();

describe("Conversations", () => {
  it("should stream deleted messages", async () => {
    const client1 = await createTestClient();
    const client2 = await createTestClient();

    // Create a group
    const conversation = await client1
      .conversations()
      .createGroupByInboxIds([client2.inboxId]);

    // Send a message
    const messageId = await conversation.sendText("Hello, world!");

    // Set up the deletion stream
    const deletedMessages: DecodedMessage[] = [];
    const stream = await client1.conversations().streamMessageDeletions({
      on_message_deleted: (message: DecodedMessage) => {
        deletedMessages.push(message);
      },
      on_error: (error: Error) => {
        console.error("Stream error:", error);
      },
      on_close: () => {
        console.log("Stream closed");
      },
    });

    // Wait for stream to be ready
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Delete the message
    const deletedCount = client1.conversations().deleteMessageById(messageId);
    expect(deletedCount).toBe(1);

    // Wait for stream to receive the deleted message
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Verify the stream received the deleted message with full details
    expect(deletedMessages.length).toBe(1);
    expect(deletedMessages[0].id).toBe(messageId);
    expect(deletedMessages[0].senderInboxId).toBe(client1.inboxId);
    expect(deletedMessages[0].conversationId).toBe(conversation.id());

    stream.end();
  });
});
