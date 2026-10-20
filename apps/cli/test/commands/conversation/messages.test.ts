import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface Message {
  id: string;
  content: { kind: string; value?: unknown };
  senderInboxId: string;
  sentAt: string;
  deliveryStatus: string;
}

describe("conversation messages", () => {
  it("lists messages in a conversation", async () => {
    const sender = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();

    // Create group and send messages
    const groupResult = await runWithIdentity(sender, [
      "conversations",
      "create-group",
      member.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    await runWithIdentity(sender, [
      "conversation",
      "send-text",
      group.id,
      "Message 1",
    ]);
    await runWithIdentity(sender, [
      "conversation",
      "send-text",
      group.id,
      "Message 2",
    ]);

    // List messages
    const listResult = await runWithIdentity(sender, [
      "conversation",
      "messages",
      group.id,
      "--json",
    ]);

    expect(listResult.exitCode).toBe(0);

    const messages = parseJsonOutput<Message[]>(listResult.stdout);
    // May include membership change messages, so check for at least 2
    expect(messages.length).toBeGreaterThanOrEqual(2);
  });
});
