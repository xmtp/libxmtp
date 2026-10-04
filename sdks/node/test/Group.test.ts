import {
  createRegisteredClient,
  createSigner,
  sleep,
  TestCodec,
} from "@test/helpers";
import {
  MessageStream,
  Timestamp,
  standardContentType,
  encodeText,
  type DisappearingSettings,
  type Message,
  type Group,
  type MessageContent,
} from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

// Background workers (self-remove, disappearing messages) complete
// asynchronously; poll until the expected state appears instead of pacing
// with fixed sleeps — a fixed sleep loses the race on loaded CI runners.
const WAIT = { timeout: 30_000, interval: 1000 };

describe("Group", () => {
  it("should create a group", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    const group = await client1.conversations.createGroup([client2.inboxId]);
    expect(group).toBeDefined();
    expect((await client1.conversations.getById(group.id))?.id).toBe(group.id);
    expect(group.id).toBeDefined();
    expect(group.createdAt).toBeDefined();
    expect(group.createdAt.ns).toBeDefined();
    expect((await group.state()).common.isActive).toBe(true);
    expect((await group.state()).name).toBe("");
    expect(group.addedByInboxId).toBe(client1.inboxId);
    expect((await group.messages()).length).toBe(1);

    const members = await group.members();
    expect(members.length).toBe(2);
    const memberInboxIds = members.map((member) => member.inboxId);
    expect(memberInboxIds).toContain(client1.inboxId);
    expect(memberInboxIds).toContain(client2.inboxId);
    expect({
      conversationType: group.kind,
      creatorInboxId: group.creatorInboxId,
    }).toEqual({
      conversationType: "group",
      creatorInboxId: client1.inboxId,
    });

    expect((await client1.conversations.listDms({})).length).toBe(0);

    const groups = await client1.conversations.listGroups({});
    expect(groups.length).toBe(1);
    expect(groups[0].id).toBe(group.id);

    // confirm group in other client
    await client2.conversations.sync();
    const groups2 = await client2.conversations.listGroups({});
    expect(groups2.length).toBe(1);
    expect(groups2[0].id).toBe(group.id);

    expect((await client2.conversations.listDms({})).length).toBe(0);
  });

  it("should create a group with an identifier", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2, identifier: identifier2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([identifier2]);
    expect(group).toBeDefined();
    expect((await client1.conversations.getById(group.id))?.id).toBe(group.id);
    expect(group.id).toBeDefined();
    expect(group.createdAt).toBeDefined();
    expect(group.createdAt.ns).toBeDefined();
    expect((await group.state()).common.isActive).toBe(true);
    expect((await group.state()).name).toBe("");
    expect(group.addedByInboxId).toBe(client1.inboxId);
    expect((await group.messages()).length).toBe(1);

    const members = await group.members();
    expect(members.length).toBe(2);
    const memberInboxIds = members.map((member) => member.inboxId);
    expect(memberInboxIds).toContain(client1.inboxId);
    expect(memberInboxIds).toContain(client2.inboxId);
    expect({
      conversationType: group.kind,
      creatorInboxId: group.creatorInboxId,
    }).toEqual({
      conversationType: "group",
      creatorInboxId: client1.inboxId,
    });

    const groups = await client1.conversations.listGroups({});
    expect(groups.length).toBe(1);
    expect(groups[0].id).toBe(group.id);
    expect((await client1.conversations.listDms({})).length).toBe(0);

    // confirm group in other client
    await client2.conversations.sync();
    const groups2 = await client2.conversations.listGroups({});
    expect(groups2.length).toBe(1);
    expect(groups2[0].id).toBe(group.id);

    expect((await client2.conversations.listDms({})).length).toBe(0);
  });

  it("should optimistically create a group", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroupOptimistic({
      name: "foo",
      description: "bar",
    });

    expect(group.id).toBeDefined();
    expect((await group.state()).name).toBe("foo");
    expect((await group.state()).description).toBe("bar");
    expect((await group.state()).imageUrl).toBe("");
    expect(group.addedByInboxId).toBe(client1.inboxId);

    const text = "gm";
    await group.sendText(text, { optimistic: true });

    const messages = await group.messages();
    expect(messages.length).toBe(1);
    expect(messages[0].content).toEqual({ kind: "text", value: text });
    expect(messages[0].deliveryStatus).toBe("unpublished");

    await group.publishMessages();

    const messages2 = await group.messages();
    expect(messages2.length).toBe(1);
    expect(messages2[0].content).toEqual({ kind: "text", value: text });
    expect(messages2[0].deliveryStatus).toBe("published");
  });

  it("should produce deterministic ids for a caller-set idempotency key", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // Same content + same key => same id, deduplicated (no new stored message).
    const id1 = await group.sendText("gm", { idempotencyKey: "key-1" });
    const countAfterFirst = (await group.messages()).length;
    const id2 = await group.sendText("gm", { idempotencyKey: "key-1" });
    expect(id2).toBe(id1);
    expect((await group.messages()).length).toBe(countAfterFirst);

    // Different key (or no key) => different id, stored as a new message.
    const id3 = await group.sendText("gm", { idempotencyKey: "key-2" });
    const id4 = await group.sendText("gm");
    expect(id3).not.toBe(id1);
    expect(id4).not.toBe(id1);
    expect(id4).not.toBe(id3);
    expect((await group.messages()).length).toBe(countAfterFirst + 2);
  });

  it("should optimistically create a group with members", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroupOptimistic({
      name: "foo",
      description: "bar",
    });

    expect(group.id).toBeDefined();
    expect((await group.state()).name).toBe("foo");
    expect((await group.state()).description).toBe("bar");
    expect((await group.state()).imageUrl).toBe("");
    expect(group.addedByInboxId).toBe(client1.inboxId);

    const text = "gm";
    await group.sendText(text, { optimistic: true });

    const messages = await group.messages();
    expect(messages.length).toBe(1);
    expect(messages[0].content).toEqual({ kind: "text", value: text });
    expect(messages[0].deliveryStatus).toBe("unpublished");

    await group.addMembers([client2.inboxId]);

    const members = await group.members();
    const memberInboxIds = members.map((member) => member.inboxId);
    expect(memberInboxIds.length).toBe(2);
    expect(memberInboxIds).toContain(client1.inboxId);
    expect(memberInboxIds).toContain(client2.inboxId);

    const messages3 = await group.messages();
    expect(messages3.length).toBe(2);
    expect(messages3[0].content).toEqual({ kind: "text", value: text });
    expect(messages3[0].deliveryStatus).toBe("published");
    expect(messages3[1].deliveryStatus).toBe("published");
  });

  it("should create a group with options", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([], {
      name: "foo",
      imageUrl: "https://foo/bar.png",
      description: "foo",
    });
    expect((await group.state()).name).toBe("foo");
    expect((await group.state()).imageUrl).toBe("https://foo/bar.png");
    expect((await group.state()).description).toBe("foo");
  });

  it("should update group name", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([]);
    expect((await group.state()).name).toBe("");
    const newName = "foo";
    await group.updateName(newName);
    expect((await group.state()).name).toBe(newName);
    const messages = await group.messages();
    expect(messages.length).toBe(1);
    const message = messages[0] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(message.content.value.metadataFieldChanges).toHaveLength(1);
    expect(message.content.value.metadataFieldChanges[0].fieldName).toBe(
      "group_name",
    );
    expect(message.content.value.metadataFieldChanges[0].oldValue).toBe("");
    expect(message.content.value.metadataFieldChanges[0].newValue).toBe(
      newName,
    );
  });

  it("should update group image URL", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([]);
    expect((await group.state()).imageUrl).toBe("");
    const imageUrl = "https://foo/bar.jpg";
    await group.updateImageUrl(imageUrl);
    expect((await group.state()).imageUrl).toBe(imageUrl);
    const messages = await group.messages();
    expect(messages.length).toBe(1);
    const message = messages[0] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(message.content.value.metadataFieldChanges).toHaveLength(1);
    expect(message.content.value.metadataFieldChanges[0].fieldName).toBe(
      "group_image_url_square",
    );
    expect(message.content.value.metadataFieldChanges[0].oldValue).toBe("");
    expect(message.content.value.metadataFieldChanges[0].newValue).toBe(
      imageUrl,
    );
  });

  it("should update group description", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([]);
    expect((await group.state()).description).toBe("");
    const newDescription = "foo";
    await group.updateDescription(newDescription);
    expect((await group.state()).description).toBe(newDescription);
    const messages = await group.messages();
    expect(messages.length).toBe(1);
    const message = messages[0] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(message.content.value.metadataFieldChanges).toHaveLength(1);
    expect(message.content.value.metadataFieldChanges[0].fieldName).toBe(
      "description",
    );
    expect(message.content.value.metadataFieldChanges[0].oldValue).toBe("");
    expect(message.content.value.metadataFieldChanges[0].newValue).toBe(
      newDescription,
    );
  });

  it("should update group app data", async () => {
    const { signer: signer1 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const group = await client1.conversations.createGroup([]);
    expect((await group.state()).appData).toBe("");
    const appData = "foo";
    await group.updateAppData(appData, undefined);
    expect((await group.state()).appData).toBe(appData);
    const messages = await group.messages();
    expect(messages.length).toBe(1);
    const message = messages[0] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(message.content.value.metadataFieldChanges).toHaveLength(1);
    expect(message.content.value.metadataFieldChanges[0].fieldName).toBe(
      "app_data",
    );
    expect(message.content.value.metadataFieldChanges[0].oldValue).toBe("");
    expect(message.content.value.metadataFieldChanges[0].newValue).toBe(
      appData,
    );
  });

  it("should send and list messages", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    expect(await group.lastMessage()).toBeDefined();

    const text = "gm";
    await group.sendText(text);

    const messages = await group.messages();
    expect(messages.length).toBe(2);
    expect(messages[1].content).toEqual({ kind: "text", value: text });

    const lastMessage = await group.lastMessage();
    expect(lastMessage).toBeDefined();
    expect(lastMessage?.id).toBe(messages[1].id);
    expect(lastMessage?.content).toEqual({ kind: "text", value: text });

    await client2.conversations.sync();
    const groups = await client2.conversations.listGroups({});
    expect(groups.length).toBe(1);

    const group2 = groups[0];
    expect(group2).toBeDefined();
    await group2.sync();
    expect(group2.id).toBe(group.id);

    const messages2 = await group2.messages();
    expect(messages2.length).toBe(2);
    expect(messages2[1].content).toEqual({ kind: "text", value: text });

    const lastMessage2 = await group2.lastMessage();
    expect(lastMessage2).toBeDefined();
    expect(lastMessage2?.id).toBe(messages2[1].id);
    expect(lastMessage2?.content).toEqual({ kind: "text", value: text });
  });

  it("should optimistically send and list messages", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    const text = "gm";
    await group.sendText(text, { optimistic: true });

    const messages = await group.messages();
    expect(messages.length).toBe(2);
    expect(messages[1].content).toEqual({ kind: "text", value: text });

    await client2.conversations.sync();
    const groups = await client2.conversations.listGroups({});
    expect(groups.length).toBe(1);

    const group2 = groups[0];
    expect(group2).toBeDefined();

    await group2.sync();
    expect(group2.id).toBe(group.id);

    const messages2 = await group2.messages();
    expect(messages2.length).toBe(1);

    await group.publishMessages();
    await group2.sync();

    const messages4 = await group2.messages();
    expect(messages4.length).toBe(2);
    expect(messages4[1].content).toEqual({ kind: "text", value: text });
  });

  it("should filter messages with options", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const testCodec = new TestCodec();
    const client1 = await createRegisteredClient(signer1, {
      codecs: [testCodec],
    });
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    // leave request message
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    await group2.requestRemoval();
    await group.sync();

    // With event-driven self-remove, client1 (super-admin) processes the
    // request via the worker, adding a removal GroupUpdated commit. Wait for
    // it up front (poll until the member is gone) so message ordering and
    // counts below are deterministic.
    await client1.conversations.syncAll(undefined);
    await vi.waitFor(async () => {
      await group.sync();
      expect((await group.members()).length).toBe(1);
    }, WAIT);

    const textMessageId = await group.sendText("gm");
    await group.sendMarkdown("# gm");
    await group.sendAttachment({
      filename: "test.txt",
      mimeType: "text/plain",
      content: new Uint8Array([1, 2, 3]),
    });
    await group.sendReply(textMessageId, client1.inboxId, encodeText("gm"));
    const replyMessage = await group.lastMessage();
    await group.sendReaction(textMessageId, client1.inboxId, {
      action: "added",
      content: "👍",
      schema: "unicode",
    });
    await group.sendActions({
      id: "actions-1",
      description: "test",
      actions: [
        {
          id: "opt-1",
          label: "Option 1",
        },
      ],
    });
    await group.sendIntent({
      id: "intent-1",
      actionId: "opt-1",
    });
    await group.sendTransactionReference({
      networkId: "1",
      reference: "1234567890",
    });
    await group.sendWalletSendCalls({
      version: "1.0",
      chainId: "1",
      from: "0x1234567890",
      calls: [
        {
          to: "0x1234567890",
          data: "0x1234567890",
          value: "0x1234567890",
        },
      ],
    });
    await group.sendReadReceipt();
    await group.sendRemoteAttachment({
      url: "https://foo/bar.png",
      contentDigest: "1234567890",
      secret: new Uint8Array([1, 2, 3]),
      salt: new Uint8Array([1, 2, 3]),
      nonce: new Uint8Array([1, 2, 3]),
      scheme: "https",
      contentLength: 100,
    });
    await group.sendMultiRemoteAttachment({
      attachments: [
        {
          url: "https://foo/bar.png",
          contentDigest: "1234567890",
          secret: new Uint8Array([1, 2, 3]),
          salt: new Uint8Array([1, 2, 3]),
          nonce: new Uint8Array([1, 2, 3]),
          scheme: "https",
          contentLength: 100,
        },
      ],
    });

    await group.send(testCodec.encode({ test: "test" }));

    const messages = await group.messages();
    // read receipts and reactions are automatically filtered; the self-remove
    // commit adds one GroupUpdated message on top of the original 13.
    expect(messages.length).toBe(14);

    // default sort order
    expect(messages[0].contentType).toEqual(
      standardContentType("groupUpdated"),
    );

    // descending sort order
    const sortedMessages1 = await group.messages({
      direction: "descending",
    });
    expect(sortedMessages1[0].contentType).toEqual(testCodec.type);

    const filteredMessages1 = await group.messages({
      contentTypes: [
        standardContentType("text"),
        standardContentType("markdown"),
        standardContentType("reply"),
      ],
    });
    expect(filteredMessages1.length).toBe(3);

    const filteredMessages2 = await group.messages({
      contentTypes: [
        standardContentType("actions"),
        standardContentType("intent"),
        standardContentType("transactionReference"),
        standardContentType("walletSendCalls"),
      ],
    });
    expect(filteredMessages2.length).toBe(4);

    const filteredMessages3 = await group.messages({
      contentTypes: [
        standardContentType("attachment"),
        standardContentType("remoteAttachment"),
        standardContentType("multiRemoteAttachment"),
      ],
    });
    expect(filteredMessages3.length).toBe(3);

    const filteredMessages4 = await group.messages({
      contentTypes: [
        standardContentType("groupUpdated"),
        standardContentType("leaveRequest"),
      ],
    });
    // Two membership changes and one removal request.
    expect(filteredMessages4.length).toBe(3);
    await expect(
      group.messages({ contentTypes: [testCodec.type] }),
    ).rejects.toMatchObject({ details: { code: "InvalidArgument" } });

    const filteredMessages5 = await group.messages({
      excludeSenderInboxIds: [client2.inboxId],
    });
    expect(filteredMessages5.length).toBe(13);

    const filteredMessages6 = await group.messages({
      excludeContentTypes: [
        standardContentType("text"),
        standardContentType("markdown"),
        standardContentType("reply"),
      ],
    });
    expect(filteredMessages6.length).toBe(11);

    const filteredMessages7 = await group.messages({
      sentAfter: replyMessage?.sentAt,
    });
    // does not include reaction and read receipt messages
    expect(filteredMessages7.length).toBe(7);

    const filteredMessages8 = await group.messages({
      sentBefore: replyMessage?.sentAt,
    });
    // includes the self-remove commit, which is processed before the reply
    expect(filteredMessages8.length).toBe(6);

    // initial add + self-remove commit
    const filteredMessages9 = await group.messages({
      kind: "membershipChange",
    });
    expect(filteredMessages9.length).toBe(2);

    await group.sendText("gm", { optimistic: true });
    const filteredMessages10 = await group.messages({
      deliveryStatus: "published",
    });
    expect(filteredMessages10.length).toBe(14);
    const filteredMessages11 = await group.messages({
      deliveryStatus: "unpublished",
    });
    expect(filteredMessages11.length).toBe(1);
  });

  it("should stream messages", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    await client2.conversations.sync();
    const groups = await client2.conversations.listGroups({});
    expect(groups.length).toBe(1);
    expect(groups[0].id).toBe(group.id);

    const cursor = (
      await groups[0].messages({ direction: "descending", limit: 1 })
    )[0]?.deliveryCursor;
    const streamedMessages: unknown[] = [];
    const stream = MessageStream.openGroup(client2, groups[0], {
      from: cursor ?? undefined,
    });
    await stream.ready();
    void stream.onValue((message) => {
      if (message.content.kind === "text")
        streamedMessages.push(message.content.value);
    });

    await group.sendText("gm");
    await group.sendText("gm2");

    await vi.waitFor(() => {
      expect(streamedMessages).toEqual(["gm", "gm2"]);
    }, WAIT);
    await stream.end();
    expect(streamedMessages).toEqual(["gm", "gm2"]);
  });

  it("should add and remove members", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const { signer: signer3 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const client3 = await createRegisteredClient(signer3);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    const members = await group.members();

    const memberInboxIds = members.map((member) => member.inboxId);
    expect(memberInboxIds).toContain(client1.inboxId);
    expect(memberInboxIds).toContain(client2.inboxId);
    expect(memberInboxIds).not.toContain(client3.inboxId);

    await group.addMembers([client3.inboxId]);

    const members2 = await group.members();
    expect(members2.length).toBe(3);

    const memberInboxIds2 = members2.map((member) => member.inboxId);
    expect(memberInboxIds2).toContain(client1.inboxId);
    expect(memberInboxIds2).toContain(client2.inboxId);
    expect(memberInboxIds2).toContain(client3.inboxId);

    await group.removeMembers([client2.inboxId]);

    const members3 = await group.members();
    expect(members3.length).toBe(2);

    const memberInboxIds3 = members3.map((member) => member.inboxId);
    expect(memberInboxIds3).toContain(client1.inboxId);
    expect(memberInboxIds3).not.toContain(client2.inboxId);
    expect(memberInboxIds3).toContain(client3.inboxId);

    const messages = (await group.messages()) as (Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    })[];
    expect(messages.length).toBe(3);
    expect(messages[0].content.value.addedInboxes).toHaveLength(1);
    expect(messages[0].content.value.addedInboxes[0]).toBe(client2.inboxId);
    expect(messages[1].content.value.addedInboxes).toHaveLength(1);
    expect(messages[1].content.value.addedInboxes[0]).toBe(client3.inboxId);
    expect(messages[2].content.value.removedInboxes).toHaveLength(1);
    expect(messages[2].content.value.removedInboxes[0]).toBe(client2.inboxId);
  });

  it("should add and remove admins", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    expect(await group.isSuperAdmin(client1.inboxId)).toBe(true);
    expect((await group.listSuperAdmins()).length).toBe(1);
    expect(await group.listSuperAdmins()).toContain(client1.inboxId);
    expect(await group.isAdmin(client1.inboxId)).toBe(false);
    expect(await group.isAdmin(client2.inboxId)).toBe(false);
    expect((await group.listAdmins()).length).toBe(0);

    await group.addAdmin(client2.inboxId);
    expect(await group.isAdmin(client2.inboxId)).toBe(true);
    expect((await group.listAdmins()).length).toBe(1);
    expect(await group.listAdmins()).toContain(client2.inboxId);

    await group.removeAdmin(client2.inboxId);
    expect(await group.isAdmin(client2.inboxId)).toBe(false);
    expect((await group.listAdmins()).length).toBe(0);

    const messages = (await group.messages()) as (Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    })[];
    expect(messages.length).toBe(3);
    expect(messages[1].content.value.addedAdminInboxes).toHaveLength(1);
    expect(messages[1].content.value.addedAdminInboxes[0]).toBe(
      client2.inboxId,
    );
    expect(messages[2].content.value.removedAdminInboxes).toHaveLength(1);
    expect(messages[2].content.value.removedAdminInboxes[0]).toBe(
      client2.inboxId,
    );
  });

  it("should add and remove super admins", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    expect(await group.isSuperAdmin(client1.inboxId)).toBe(true);
    expect(await group.isSuperAdmin(client2.inboxId)).toBe(false);
    expect((await group.listSuperAdmins()).length).toBe(1);
    expect(await group.listSuperAdmins()).toContain(client1.inboxId);

    await group.addSuperAdmin(client2.inboxId);
    expect(await group.isSuperAdmin(client2.inboxId)).toBe(true);
    expect((await group.listSuperAdmins()).length).toBe(2);
    expect(await group.listSuperAdmins()).toContain(client1.inboxId);
    expect(await group.listSuperAdmins()).toContain(client2.inboxId);

    await group.removeSuperAdmin(client2.inboxId);
    expect(await group.isSuperAdmin(client2.inboxId)).toBe(false);
    expect((await group.listSuperAdmins()).length).toBe(1);
    expect(await group.listSuperAdmins()).toContain(client1.inboxId);

    const messages = (await group.messages()) as (Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    })[];
    expect(messages.length).toBe(3);
    expect(messages[1].content.value.addedSuperAdminInboxes).toHaveLength(1);
    expect(messages[1].content.value.addedSuperAdminInboxes[0]).toBe(
      client2.inboxId,
    );
    expect(messages[2].content.value.removedSuperAdminInboxes).toHaveLength(1);
    expect(messages[2].content.value.removedSuperAdminInboxes[0]).toBe(
      client2.inboxId,
    );
  });

  it("should manage consent state", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);
    expect(group).toBeDefined();
    expect((await group.state()).common.consentState).toBe("allowed");

    await client2.conversations.sync();
    const group2 = (await client2.conversations.getById(group.id)) as Group;
    expect(group2).toBeDefined();
    expect((await group2.state()).common.consentState).toBe("unknown");
    await group2!.sendText("gm!");
    expect((await group2.state()).common.consentState).toBe("allowed");
  });

  it("should handle disappearing messages", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    const stream = await client1.events({
      kinds: ["message.expired"],
      referencesOwnMessages: false,
    });

    // create message disappearing settings so that messages are deleted after 1 second
    const messageDisappearingSettings: DisappearingSettings = {
      from: new Timestamp(1n),
      retentionNs: 2_000_000_000n,
    };

    // create a group with message disappearing settings
    const group = await client1.conversations.createGroup([client2.inboxId], {
      disappearing: messageDisappearingSettings,
    });

    // verify that the message disappearing settings are set and enabled
    expect((await group.state()).common.disappearingSettings).toEqual({
      from: new Timestamp(1n),
      retentionNs: 2_000_000_000n,
    });
    expect((await group.state()).common.isDisappearingEnabled).toBe(true);

    // send messages to the group
    const messageId1 = await group.sendText("gm");
    const messageId2 = await group.sendText("gm2");

    // verify that the messages are sent
    expect((await group.messages()).length).toBe(3);

    // sync the messages to the other client
    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];
    await group2.sync();

    // verify that the message disappearing settings are set and enabled
    expect((await group2.state()).common.disappearingSettings).toEqual({
      from: new Timestamp(1n),
      retentionNs: 2_000_000_000n,
    });
    expect((await group2.state()).common.isDisappearingEnabled).toBe(true);

    // poll until the disappearing-messages worker deletes the expired
    // messages
    await vi.waitFor(async () => {
      expect((await group.messages()).length).toBe(1);
    }, WAIT);

    // verify that the messages are deleted on the other client
    expect((await group2.messages()).length).toBe(1);

    setTimeout(() => {
      void stream.return();
    }, 1000);

    let count = 0;
    const messageIds: string[] = [];
    for await (const message of stream) {
      count++;
      expect(message).toBeDefined();
      if (message.kind === "message.expired")
        messageIds.push(message.messageId);
    }
    expect(count).toBe(2);
    expect(messageIds).toContain(messageId1);
    expect(messageIds).toContain(messageId2);

    // remove the message disappearing settings
    await group.updateDisappearingSettings(undefined);

    // verify that the message disappearing settings are removed
    expect((await group.state()).common.disappearingSettings).toEqual({
      from: new Timestamp(0n),
      retentionNs: 0n,
    });

    expect((await group.state()).common.isDisappearingEnabled).toBe(false);

    // sync other group
    await group2.sync();

    // verify that the message disappearing settings are set and disabled
    expect((await group2.state()).common.disappearingSettings).toEqual({
      from: new Timestamp(0n),
      retentionNs: 0n,
    });
    expect((await group2.state()).common.isDisappearingEnabled).toBe(false);

    // check for metadata field changes
    const messages = await group2.messages();
    const fieldChange1 = messages[1] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(fieldChange1.content.value.metadataFieldChanges).toBeDefined();
    expect(fieldChange1.content.value.metadataFieldChanges.length).toBe(1);
    expect(fieldChange1.content.value.metadataFieldChanges[0].fieldName).toBe(
      "message_disappear_from_ns",
    );
    expect(fieldChange1.content.value.metadataFieldChanges[0].oldValue).toBe(
      "1",
    );
    expect(fieldChange1.content.value.metadataFieldChanges[0].newValue).toBe(
      "0",
    );

    const fieldChange2 = messages[2] as Message & {
      content: Extract<MessageContent, { kind: "groupUpdated" }>;
    };
    expect(fieldChange2.content.value.metadataFieldChanges).toBeDefined();
    expect(fieldChange2.content.value.metadataFieldChanges.length).toBe(1);
    expect(fieldChange2.content.value.metadataFieldChanges[0].fieldName).toBe(
      "message_disappear_in_ns",
    );
    expect(fieldChange2.content.value.metadataFieldChanges[0].oldValue).toBe(
      "2000000000",
    );
    expect(fieldChange2.content.value.metadataFieldChanges[0].newValue).toBe(
      "0",
    );

    // send messages to the group
    await group2.sendText("gm");
    await group2.sendText("gm2");

    // verify that the messages are sent
    expect((await group2.messages()).length).toBe(5);

    // sync original group
    await group.sync();

    // verify that the messages are not deleted
    expect((await group.messages()).length).toBe(5);
  });

  it("should count messages with various filters", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    const group = await client1.conversations.createGroup([client2.inboxId]);

    await group.sendText("text 1");
    await sleep(10);
    const timestamp1 = BigInt(Date.now() * 1_000_000);
    await sleep(10);
    await group.sendText("text 2");
    await sleep(10);
    const timestamp2 = BigInt(Date.now() * 1_000_000);
    await sleep(10);
    await group.sendText("text 3");

    expect(await group.countMessages({})).toBe(4n);

    // Time filters
    expect(
      await group.countMessages({
        sentBefore: new Timestamp(timestamp1),
        contentTypes: [standardContentType("text")],
      }),
    ).toBe(1n);
    expect(
      await group.countMessages({
        sentAfter: new Timestamp(timestamp1),
      }),
    ).toBe(2n);
    expect(
      await group.countMessages({
        sentAfter: new Timestamp(timestamp2),
        contentTypes: [standardContentType("text")],
      }),
    ).toBe(1n);
    expect(
      await group.countMessages({
        sentAfter: new Timestamp(timestamp1),
        sentBefore: new Timestamp(timestamp2),
      }),
    ).toBe(1n);

    // Content type filter
    expect(
      await group.countMessages({
        contentTypes: [standardContentType("text")],
      }),
    ).toBe(3n);
  });

  it("should have pending removal state after requesting removal from the group", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    await client1.conversations.createGroup([client2.inboxId]);

    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];

    expect((await group2.state()).membershipState === "pendingRemove").toBe(
      false,
    );
    await group2.requestRemoval();
    expect((await group2.state()).membershipState === "pendingRemove").toBe(
      true,
    );
    expect((await group2.state()).common.isActive).toBe(true);

    const messages = await group2.messages();
    const leaveRequestMessage = messages[1];
    expect(leaveRequestMessage.contentType).toEqual(
      standardContentType("leaveRequest"),
    );
  });

  it("should remove a member after processing their removal request", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId]);

    await client2.conversations.sync();
    const group2 = (await client2.conversations.listGroups({}))[0];

    await group2.requestRemoval();

    // messages and welcomes must be synced
    await client2.conversations.syncAll(undefined);

    // The removal worker publishes before either client must process the commit.
    // Wait for both clients to apply it.
    await vi.waitFor(async () => {
      await client1.conversations.syncAll(undefined);
      await group2.sync();
      expect((await group2.state()).common.isActive).toBe(false);
      expect(await group.members()).toHaveLength(1);
      expect(await group2.members()).toHaveLength(1);
    }, WAIT);
    expect((await group2.state()).membershipState === "pendingRemove").toBe(
      true,
    );
  });
});
