import { createRegisteredClient, createSigner } from "@test/helpers";
import { ConversationStream, MessageStream } from "@xmtp/node-sdk";
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

  it("should not have initial conversations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    expect((await client.conversations.list()).length).toBe(0);
    expect((await client.conversations.listDms(undefined)).length).toBe(0);
    expect((await client.conversations.listGroups(undefined)).length).toBe(0);
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

  it("should get a DM by inbox ID", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const dm = await client1.conversations.createDm(client2.inboxId);
    const foundDm = await client1.conversations.getDmByInboxId(client2.inboxId);
    expect(foundDm).toBeDefined();
    expect(foundDm!.id).toBe(dm.id);
  });

  it("should get a DM by identifier", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2, identifier: identifier2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const dm = await client1.conversations.createDm(client2.inboxId);
    const foundDm = await client1.conversations.getDmByIdentity(identifier2);
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

  it("should list conversations with options", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    const group1 = await client1.conversations.createGroup([client2.inboxId]);
    const group2 = await client1.conversations.createGroup([client3.inboxId]);
    const dm1 = await client1.conversations.createDm(client2.inboxId);
    const dm2 = await client1.conversations.createDm(client3.inboxId);

    // conversations by type (group)
    const groups = await client1.conversations.list({
      kind: "group",
    });
    expect(groups.length).toBe(2);
    expect(groups[0].id).toBe(group2.id);
    expect(groups[1].id).toBe(group1.id);

    // conversations by type (dm)
    const dms = await client1.conversations.list({
      kind: "dm",
    });
    expect(dms.length).toBe(2);
    expect(dms[0].id).toBe(dm2.id);
    expect(dms[1].id).toBe(dm1.id);

    // conversations by created before timestamp
    const convos = await client1.conversations.list({
      createdBefore: dm2.createdAt,
    });
    expect(convos.length).toBe(3);
    expect(convos[0].id).toBe(dm1.id);
    expect(convos[1].id).toBe(group2.id);
    expect(convos[2].id).toBe(group1.id);

    // conversations by created after timestamp
    const convos2 = await client1.conversations.list({
      createdAfter: group1.createdAt,
    });
    expect(convos2.length).toBe(3);
    expect(convos2[0].id).toBe(dm2.id);
    expect(convos2[1].id).toBe(dm1.id);
    expect(convos2[2].id).toBe(group2.id);

    // conversations by created after timestamp and before timestamp
    const convos3 = await client1.conversations.list({
      createdBefore: dm2.createdAt,
      createdAfter: group1.createdAt,
    });
    expect(convos3.length).toBe(2);
    expect(convos3[0].id).toBe(dm1.id);
    expect(convos3[1].id).toBe(group2.id);

    // conversations by limit
    const convos4 = await client1.conversations.list({
      limit: 1,
      orderBy: "createdAt",
    });
    expect(convos4.length).toBe(1);
    expect(convos4[0].id).toBe(dm2.id);

    // conversations by order by (created at)
    const convos5 = await client1.conversations.list({
      limit: 1,
      orderBy: "lastActivity",
    });
    expect(convos5.length).toBe(1);
    expect(convos5[0].id).toBe(dm2.id);
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
  it.each([undefined, "group", "dm"] as const)(
    "streams messages for selected conversation kind %s",
    async (conversationKind) => {
      const client = await createRegisteredClient(createSigner().signer);
      const peer = await createRegisteredClient(createSigner().signer);
      const group = await client.conversations.createGroup([peer.inboxId]);
      const dm = await client.conversations.createDm(peer.inboxId);
      const stream = MessageStream.open(client, { conversationKind });
      await stream.ready();
      const seen: string[] = [];
      const consumed = stream.onValue((message) => {
        if (message.content.kind === "text") seen.push(message.id);
      });
      try {
        const groupId = await group.sendText("group");
        const dmId = await dm.sendText("dm");
        const expected =
          conversationKind === "group"
            ? [groupId]
            : conversationKind === "dm"
              ? [dmId]
              : [groupId, dmId];
        await vi.waitFor(() => expect(seen).toEqual(expected));
      } finally {
        await stream.end();
        await consumed;
      }
    },
  );

  it("should get hmac keys", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);
    const dm = await client1.conversations.createDm(client2.inboxId);
    const hmacKeys = await client1.conversations.hmacKeys();
    expect(hmacKeys).toBeDefined();
    const keys = [...hmacKeys.keys()];
    expect(keys.length).toBe(2);
    expect(keys).toContain(group.id);
    expect(keys).toContain(dm.id);
    for (const values of hmacKeys.values()) {
      expect(values.length).toBe(3);
      for (const value of values) {
        expect(value.key).toBeDefined();
        expect(value.key.length).toBe(42);
        expect(value.epoch).toBeDefined();
        expect(typeof value.epoch).toBe("bigint");
      }
    }
    for (const conversation of [group, dm]) {
      const values = await conversation.hmacKeys();
      expect(values.length).toBe(3);
      for (const value of values) {
        expect(value.key).toBeDefined();
        expect(value.epoch).toBeDefined();
      }
    }
  });

  it("should sync groups across installations", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2, {});
    await createRegisteredClient(signer2);

    const group = await client.conversations.createGroup([client2.inboxId]);
    await client2.conversations.sync();
    const convos = await client2.conversations.listGroups(undefined);
    expect(convos.length).toBe(1);
    expect(convos[0].id).toBe(group.id);

    const group2 = await client.conversations.createDm(client2.inboxId);
    await client2.conversations.sync();
    const convos2 = await client2.conversations.list();
    expect(convos2.length).toBe(2);
    const convos2Ids = convos2.map((c) => c.id);
    expect(convos2Ids).toContain(group.id);
    expect(convos2Ids).toContain(group2.id);
  });

  it("should stitch DM groups together", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    // Create both physical DMs before either client receives the peer's Welcome.
    const client1 = await createRegisteredClient(signer1, {
      deviceSync: false,
    });
    const client2 = await createRegisteredClient(signer2, {
      deviceSync: false,
    });
    const dm1 = await client1.conversations.createDm(client2.inboxId);
    const dm2 = await client2.conversations.createDm(client1.inboxId);
    expect(dm1.id).not.toBe(dm2.id);

    await dm1.sendText("hi");
    // since this is the last message sent, the stitched group ID will be
    // this group ID
    await dm2.sendText("hi");

    await client1.conversations.sync();
    await client2.conversations.sync();
    await dm1.sync();
    await dm2.sync();

    const dm1_2 = await client1.conversations.getById(dm1.id);
    const dm2_2 = await client2.conversations.getById(dm2.id);
    expect(dm1_2?.id).toBe(dm2.id);
    expect(dm2_2?.id).toBe(dm2.id);

    const dms1 = await client1.conversations.listDms(undefined);
    const dms2 = await client2.conversations.listDms(undefined);
    expect(dms1[0].id).toBe(dm2.id);
    expect(dms2[0].id).toBe(dm2.id);

    const dupeDms1 = await dms1[0].duplicateDms();
    const dupeDms2 = await dms2[0].duplicateDms();
    expect(dupeDms1.length).toBe(1);
    expect(dupeDms2.length).toBe(1);
    expect(dupeDms1[0].id).toBe(dm1.id);
    expect(dupeDms2[0].id).toBe(dm1.id);
  });
});
