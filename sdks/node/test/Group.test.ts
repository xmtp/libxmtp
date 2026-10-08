import { createRegisteredClient, createSigner } from "@test/helpers";
import { standardContentType } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

// Stream delivery completes asynchronously; poll until the expected state
// appears instead of pacing with fixed sleeps.
const WAIT = { timeout: 30_000, interval: 1000 };

// Group smoke tests. Rust owns group logic: membership, admin lists,
// permissions, consent, disappearing messages, message filters and counts.
// See `crates/xmtp_mls/src/groups/tests` and `crates/xmtp_sdk/src/tests`.
describe("Group", () => {
  it("sends, lists, and streams messages across two group creation forms", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2, identifier: identifier2 } = createSigner();
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

    // Generated conversions that Rust tests cannot see: the group update
    // body, the membership state enum, the sync summary, and each
    // camelCase message list option.
    expect(messages[0].content).toMatchObject({
      kind: "groupUpdated",
      value: {
        initiatedByInboxId: client1.inboxId,
        addedInboxes: [client2.inboxId],
        removedInboxes: [],
      },
    });
    expect((await group.state()).membershipState).toBe("allowed");
    expect((await group2.state()).membershipState).toBe("pending");
    expect(await client2.conversations.syncAll(["unknown"])).toEqual({
      eligible: 1n,
      synced: 1n,
    });
    const [updateId, textId] = messages.map((message) => message.id);
    const ids = async (options: Parameters<typeof group.messages>[0]) =>
      (await group.messages(options)).map((message) => message.id);
    const textType = standardContentType("text");
    expect(await ids({ contentTypes: [textType] })).toEqual([textId]);
    expect(await ids({ excludeContentTypes: [textType] })).toEqual([updateId]);
    expect(await ids({ excludeSenderInboxIds: [client1.inboxId] })).toEqual([]);
    expect(await ids({ kind: "membershipChange" })).toEqual([updateId]);
    expect(await ids({ sentAfter: messages[0].sentAt })).toEqual([textId]);
    expect(await ids({ sentBefore: messages[1].sentAt })).toEqual([updateId]);
    expect(await group.countMessages({ contentTypes: [textType] })).toBe(1n);

    // Send options: an optimistic send stays unpublished until
    // publishMessages(), and one idempotency key gives one message.
    const draftId = await group.sendText("draft", { optimistic: true });
    expect(await ids({ deliveryStatus: "unpublished" })).toEqual([draftId]);
    expect(
      (await client1.conversations.getMessageById(draftId))?.deliveryStatus,
    ).toBe("unpublished");
    await group.publishMessages();
    expect(await ids({ deliveryStatus: "unpublished" })).toEqual([]);
    expect(
      (await client1.conversations.getMessageById(draftId))?.deliveryStatus,
    ).toBe("published");
    const keyed = await group.sendText("keyed", { idempotencyKey: "key-1" });
    expect(await group.sendText("keyed", { idempotencyKey: "key-1" })).toBe(
      keyed,
    );
    expect(await group.sendText("keyed", { idempotencyKey: "key-2" })).not.toBe(
      keyed,
    );

    // An identity list routes to the generated createGroupWithIdentities call.
    const identityGroup = await client1.conversations.createGroup([
      identifier2,
    ]);
    expect(
      (await identityGroup.members()).map((member) => member.inboxId).sort(),
    ).toEqual([client1.inboxId, client2.inboxId].sort());

    await client2.conversations.sync();
    const streamedGroups = await client2.conversations.listGroups({});
    expect(streamedGroups).toHaveLength(2);
    expect(streamedGroups.map((item) => item.id)).toContain(group.id);
    const streamGroup = streamedGroups.find(
      (item) => item.id === identityGroup.id,
    );
    expect(streamGroup).toBeDefined();

    const cursor = (
      await streamGroup!.messages({ direction: "descending", limit: 1 })
    )[0]?.deliveryCursor;
    const streamedMessages: unknown[] = [];
    const stream = streamGroup!.streamMessages({ from: cursor ?? undefined });
    await stream.ready();
    void stream.onValue((message) => {
      if (message.content.kind === "text")
        streamedMessages.push(message.content.value);
    });

    await group.sendText("outside stream");
    await identityGroup.sendText("gm");
    await identityGroup.sendText("gm2");

    await vi.waitFor(() => {
      expect(streamedMessages).toEqual(["gm", "gm2"]);
    }, WAIT);
    await stream.end();
    expect(streamedMessages).toEqual(["gm", "gm2"]);
  });
});
