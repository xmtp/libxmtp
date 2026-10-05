import { createRegisteredClient, createSigner } from "@test/helpers";
import { ConversationStream } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

describe("Conversations", () => {
  it("should expose topic and debug info for groups and DMs", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);
    const dm = await client1.conversations.createDm(client2.inboxId);
    for (const conversation of [group, dm]) {
      expect(conversation.topic).toBe(`[group_message_v1/${conversation.id}]`);
      const debugInfo = await conversation.debugInfo();
      expect(debugInfo).toBeDefined();
      expect(debugInfo.epoch).toBeDefined();
      expect(debugInfo.maybeForked).toBe(false);
      expect(debugInfo.forkDetails).toBe("");
      expect([true, false, undefined]).toContain(debugInfo.isCommitLogForked);
      expect(debugInfo.localCommitLog).toBeDefined();
      expect(debugInfo.remoteCommitLog).toBeDefined();
      expect(debugInfo.cursor).toBeDefined();
      expect(debugInfo.cursor.length).toBeGreaterThan(0);
      for (const cursor of debugInfo.cursor) {
        expect(typeof cursor).toBe("bigint");
      }
    }
  });

  it("should get a group or DM by ID", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    const group = await client1.conversations.createGroup([client2.inboxId]);
    expect(group).toBeDefined();
    expect(group.id).toBeDefined();
    const foundGroup = await client1.conversations.getById(group.id);
    expect(foundGroup).toBeDefined();
    expect(foundGroup!.id).toBe(group.id);

    const dm = await client1.conversations.createDm(client2.inboxId);
    expect(dm).toBeDefined();
    expect(dm.id).toBeDefined();
    const foundDm = await client1.conversations.getById(dm.id);
    expect(foundDm).toBeDefined();
    expect(foundDm!.id).toBe(dm.id);
  });

  it("should get a message by ID", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);
    const messageId = await group.sendText("gm!");
    expect(messageId).toBeDefined();

    const message = await client1.conversations.getMessageById(messageId);
    expect(message).toBeDefined();
    expect(message!.id).toBe(messageId);
  });

  it.each([undefined, "group", "dm"] as const)(
    "streams selected conversation kind %s",
    async (kind) => {
      const client = await createRegisteredClient(createSigner().signer);
      const peer = await createRegisteredClient(createSigner().signer);
      const stream = ConversationStream.open(client, { kind });
      await stream.ready();
      const seen: string[] = [];
      const consumed = stream.onValue((conversation) => {
        seen.push(conversation.id);
      });
      try {
        const group = await client.conversations.createGroup([peer.inboxId]);
        const dm = await client.conversations.createDm(peer.inboxId);
        const expected =
          kind === "group"
            ? [group.id]
            : kind === "dm"
              ? [dm.id]
              : [group.id, dm.id];
        await vi.waitFor(() => expect(seen).toEqual(expected));
      } finally {
        await stream.end();
        await consumed;
      }
    },
  );
});
