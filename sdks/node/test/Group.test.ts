import { createRegisteredClient, createSigner } from "@test/helpers";
import { MessageStream } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

// Stream delivery completes asynchronously; poll until the expected state
// appears instead of pacing with fixed sleeps.
const WAIT = { timeout: 30_000, interval: 1000 };

// Group smoke tests. Rust owns group logic: membership, admin lists,
// permissions, consent, disappearing messages, message filters and counts.
// See `crates/xmtp_mls/src/groups/tests` and `crates/xmtp_sdk/src/tests`.
describe("Group", () => {
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
});
