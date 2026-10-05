import { ConversationStream, MessageStream } from "@xmtp/browser-sdk";
import { expect, test, vi } from "vitest";

import { create, signer } from "./helpers";

test("group membership keeps creator allowed and invited peer pending", async () => {
  const creator = await create();
  const peer = await create();
  const group = await creator.conversations.createGroup([peer.inboxId]);
  expect((await group.state()).membershipState).toBe("allowed");
  await peer.conversations.sync();
  const received = await peer.conversations.getById(group.id);
  if (received?.kind !== "group") throw new Error("Peer group missing");
  const invited = await received.state();
  if (!("membershipState" in invited)) throw new Error("Group state missing");
  expect(invited.membershipState).toBe("pending");
});

test("conversation callbacks retain both local and incoming group ids", async () => {
  const client = await create();
  const peer = await create();
  const other = await create();
  const stream = ConversationStream.open(client);
  await stream.ready();
  const ids: string[] = [];
  const read = stream.onValue((conversation) => {
    ids.push(conversation.id);
  });
  try {
    const local = await client.conversations.createGroup([peer.inboxId]);
    const incoming = await peer.conversations.createGroup([
      client.inboxId,
      other.inboxId,
    ]);
    const another = await other.conversations.createGroup([
      client.inboxId,
      peer.inboxId,
    ]);
    await vi.waitFor(
      () => {
        expect(ids).toEqual(
          expect.arrayContaining([local.id, incoming.id, another.id]),
        );
      },
      { timeout: 30_000 },
    );
    expect(ids.filter((id) => id === local.id)).toHaveLength(1);
    expect(ids.filter((id) => id === incoming.id)).toHaveLength(1);
    expect(ids.filter((id) => id === another.id)).toHaveLength(1);
  } finally {
    await stream.end();
    await read;
  }
});

test("default message reader includes allowed and unknown consent and excludes denied", async () => {
  const sender = await create();
  const peer = await create();
  const allowed = await sender.conversations.createGroup([peer.inboxId]);
  const unknown = await sender.conversations.createGroup([peer.inboxId]);
  const denied = await sender.conversations.createGroup([peer.inboxId]);
  await peer.conversations.syncAll(undefined);
  await peer.preferences.setConsentStates([
    {
      entity: { kind: "conversation", conversationId: allowed.id },
      state: "allowed",
    },
    {
      entity: { kind: "conversation", conversationId: unknown.id },
      state: "unknown",
    },
    {
      entity: { kind: "conversation", conversationId: denied.id },
      state: "denied",
    },
  ]);
  const deniedId = await denied.sendText("denied", { shouldPush: false });
  const allowedId = await allowed.sendText("allowed", { shouldPush: false });
  const unknownId = await unknown.sendText("unknown", { shouldPush: false });
  await peer.conversations.syncAll(undefined);
  const stream = MessageStream.open(peer);
  await stream.ready();
  const ids: string[] = [];
  const read = stream.onValue((message) => {
    ids.push(message.id);
  });
  try {
    const barrier = await allowed.sendText("after stored rows", {
      shouldPush: false,
    });
    await vi.waitFor(
      () => {
        expect(ids).toContain(allowedId);
        expect(ids).toContain(unknownId);
        expect(ids).toContain(barrier);
      },
      { timeout: 30_000 },
    );
    expect(ids).not.toContain(deniedId);
  } finally {
    await stream.end();
    await read;
  }
});

test("a running DM reader receives a new peer installation without receiver sync", async () => {
  const receiver = await create();
  const owner = signer();
  const first = await create(owner);
  const firstDm = await first.conversations.createDm(receiver.inboxId);
  const stream = MessageStream.open(receiver, { conversationKind: "dm" });
  await stream.ready();
  const ids: string[] = [];
  const read = stream.onValue((message) => {
    ids.push(message.id);
  });
  try {
    const firstId = await firstDm.sendText("first installation", {
      shouldPush: false,
    });
    await vi.waitFor(() => expect(ids).toContain(firstId), { timeout: 30_000 });
    const second = await create(owner);
    expect(second.inboxId).toBe(first.inboxId);
    expect(second.installationId).not.toBe(first.installationId);
    const secondDm = await second.conversations.createDm(receiver.inboxId);
    const secondId = await secondDm.sendText("new installation", {
      shouldPush: false,
    });
    await vi.waitFor(() => expect(ids).toContain(secondId), {
      timeout: 30_000,
    });
    expect(ids.filter((id) => id === firstId)).toHaveLength(1);
    expect(ids.filter((id) => id === secondId)).toHaveLength(1);
    expect(
      (await receiver.conversations.getMessageById(secondId))?.content,
    ).toEqual({ kind: "text", value: "new installation" });
    const history = await receiver.conversations.messageHistorySnapshot(100);
    expect(ids).toEqual(history.messages.map((message) => message.id));
    expect(
      history.messages
        .filter((message) => message.kind === "application")
        .map((message) => message.id),
    ).toEqual([firstId, secondId]);
    expect(
      [
        ...new Set(
          history.messages
            .filter((message) => message.kind === "membershipChange")
            .map((message) => message.conversationId),
        ),
      ].sort(),
    ).toEqual([...new Set([firstDm.id, secondDm.id])].sort());
  } finally {
    await stream.end();
    await read;
  }
});
