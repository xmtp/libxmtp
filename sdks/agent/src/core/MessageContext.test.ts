import { encodeText, type MessageContent } from "@xmtp/node-sdk";
import { describe, expect, expectTypeOf, it } from "vitest";

import { createClient } from "@/util/test";

import { MessageContext } from "./MessageContext";
describe("MessageContext", () => {
  it("keeps the reply body and parent in the native message", async () => {
    const client = await createClient();
    const group = await client.conversations.createGroup([]);
    const parent = await group.sendText("parent");
    const id = await group.sendReply(
      parent,
      client.inboxId,
      encodeText("reply"),
      { shouldPush: false },
    );
    const message = (await client.conversations.getMessageById(id))!;
    const ctx = new MessageContext({ message, conversation: group, client });
    expect(ctx.isReply()).toBe(true);
    if (ctx.isReply()) {
      expectTypeOf(ctx.content).toEqualTypeOf<
        Extract<MessageContent, { kind: "reply" }>
      >();
      expect(ctx.content.referenceId).toBe(parent);
      expect(ctx.content.body).toEqual({ kind: "text", value: "reply" });
    }
    await client.end();
  });
});
