import { Timestamp, type ClientEvent } from "@xmtp/browser-sdk";
import {
  AttachmentCodec,
  initPureWasm,
  TextCodec,
} from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test, vi } from "vitest";

import { create } from "./helpers";

beforeAll(() => initPureWasm());

test("message list and lookup retain identity, kind, delivery, and ordered limits", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  const ids = [];
  for (const text of ["first", "second", "third"])
    ids.push(await group.sendText(text, { shouldPush: false }));
  await peer.conversations.syncAll(undefined);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Peer group missing");
  const ascending = await received.messages({
    kind: "application",
    direction: "ascending",
  });
  expect(ascending.map((message) => message.id)).toEqual(ids);
  expect(
    (
      await received.messages({
        kind: "application",
        direction: "descending",
        limit: 2,
      })
    ).map((message) => message.id),
  ).toEqual([ids[2], ids[1]]);
  expect(
    (await received.messages({ kind: "membershipChange" })).length,
  ).toBeGreaterThan(0);
  for (const message of ascending) {
    expect(message.conversationId).toBe(group.id);
    expect(message.senderInboxId).toBe(sender.inboxId);
    expect(message.sentAt).toBeInstanceOf(Timestamp);
    expect(message.insertedAt).toBeInstanceOf(Timestamp);
    expect(message.kind).toBe("application");
    expect(message.deliveryStatus).toBe("published");
    expect(message.contentType).toMatchObject({
      authorityId: "xmtp.org",
      typeId: "text",
    });
    expect(message.fallback).toBeUndefined();
    expect(message.rawBytes.length).toBeGreaterThan(0);
    expect(await peer.conversations.getMessageById(message.id)).toMatchObject({
      id: message.id,
      senderInboxId: sender.inboxId,
      conversationId: group.id,
      content: message.content,
    });
  }
});

test("failed standard decode keeps fixed bytes and cause on list, lookup, and reply parent", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  class BrokenText extends TextCodec {
    override encode(value: string) {
      return { ...super.encode(value), content: new Uint8Array([0xff, 0xfe]) };
    }
  }
  // Fixed protobuf fixture: text type, UTF-8 parameter, and invalid UTF-8 bytes.
  const raw = new Uint8Array([
    10, 18, 10, 8, 120, 109, 116, 112, 46, 111, 114, 103, 18, 4, 116, 101, 120,
    116, 24, 1, 18, 17, 10, 8, 101, 110, 99, 111, 100, 105, 110, 103, 18, 5, 85,
    84, 70, 45, 56, 34, 2, 255, 254,
  ]);
  const brokenId = await group.send(new BrokenText(), "ignored", {
    shouldPush: false,
  });
  const afterId = await group.sendText("after", { shouldPush: false });
  await peer.conversations.syncAll(undefined);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Peer group missing");
  const replyId = await received.sendReply(
    brokenId,
    sender.inboxId,
    new TextCodec().encode("reply to bytes"),
    { shouldPush: false },
  );
  await group.sync();
  for (const [client, conversation] of [
    [sender, group],
    [peer, received],
  ] as const) {
    const listed = await conversation.messages();
    const byId = await client.conversations.getMessageById(brokenId);
    const reply = listed.find((message) => message.id === replyId);
    expect(reply?.inReplyTo?.id).toBe(brokenId);
    for (const message of [
      listed.find((item) => item.id === brokenId),
      byId,
      reply?.inReplyTo,
    ]) {
      expect(message?.rawBytes).toEqual(raw);
      expect(message?.contentType?.typeId).toBe("text");
      expect(message?.content).toMatchObject({
        kind: "unknown",
        rawBytes: raw,
        error: { code: "CodecDecodeFailed" },
      });
    }
    expect(listed.find((message) => message.id === afterId)?.content).toEqual({
      kind: "text",
      value: "after",
    });
  }
});

test("local deletion delivers one exact public event and removes the local message", async () => {
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const id = await group.sendText("delete locally", { shouldPush: false });
  expect((await client.conversations.getMessageById(id))?.senderInboxId).toBe(
    client.inboxId,
  );
  const events: ClientEvent[] = [];
  const listener = await client.startListener(
    { kinds: ["message.deleted"], referencesOwnMessages: true },
    (event) => {
      events.push(event);
    },
  );
  try {
    await client.conversations.deleteMessageLocally(id);
    await vi.waitFor(() => expect(events).toHaveLength(1), { timeout: 10_000 });
    expect(events[0]).toEqual({
      kind: "message.deleted",
      message_deleted: {
        conversationId: group.id,
        messageId: id,
        cause: "deleted_locally",
      },
    });
    expect(await client.conversations.getMessageById(id)).toBeUndefined();
    expect((await group.messages()).some((message) => message.id === id)).toBe(
      false,
    );
  } finally {
    await client.stopListener(listener);
  }
});

test("peer reaction and reply reads keep exact references, bodies, and parents", async () => {
  const sender = await create();
  const peer = await create();
  const group = await sender.conversations.createGroup([peer.inboxId]);
  const parent = await group.sendText("parent", { shouldPush: false });
  const reactions = [];
  for (const action of ["added", "removed"] as const) {
    for (const schema of ["unicode", "shortcode", "custom"] as const) {
      const reaction = { content: "reaction", action, schema };
      const id = await group.sendReaction(parent, sender.inboxId, reaction, {
        shouldPush: false,
      });
      reactions.push({ id, reaction });
    }
  }
  const attachment = {
    filename: "reply.png",
    mimeType: "image/png",
    content: new Uint8Array([4, 5, 6]),
  };
  const replies = [
    {
      id: await group.sendReply(
        parent,
        sender.inboxId,
        new TextCodec().encode("text reply"),
        { shouldPush: false },
      ),
      body: { kind: "text", value: "text reply" },
    },
    {
      id: await group.sendReply(
        parent,
        sender.inboxId,
        new AttachmentCodec().encode(attachment),
        { shouldPush: false },
      ),
      body: { kind: "attachment", value: attachment },
    },
  ];
  await peer.conversations.syncAll(undefined);
  const received = await peer.conversations.getById(group.id);
  if (!received) throw new Error("Peer group missing");
  const listed = await received.messages();
  for (const { id, reaction } of reactions) {
    const message = await peer.conversations.getMessageById(id);
    expect(message?.content).toEqual({
      kind: "reaction",
      reference: parent,
      referenceInboxId: sender.inboxId,
      reaction,
    });
    expect(message?.contentType).toMatchObject({
      authorityId: "xmtp.org",
      typeId: "reaction",
    });
    // History attaches reactions to their parent and omits reaction rows.
    expect(listed.some((item) => item.id === id)).toBe(false);
  }
  for (const { id, body } of replies) {
    for (const message of [
      await peer.conversations.getMessageById(id),
      listed.find((item) => item.id === id),
    ]) {
      expect(message?.content).toEqual({
        kind: "reply",
        referenceId: parent,
        body,
      });
      expect(message?.inReplyTo).toMatchObject({
        id: parent,
        senderInboxId: sender.inboxId,
        content: { kind: "text", value: "parent" },
      });
      expect(message?.contentType).toMatchObject({
        authorityId: "xmtp.org",
        typeId: "reply",
      });
    }
  }
});

test("a caller idempotency key keeps exact message ids and stored counts", async () => {
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const first = await group.sendText("same", {
    idempotencyKey: "first",
    shouldPush: false,
  });
  const count = (await group.messages()).length;
  expect(
    await group.sendText("same", {
      idempotencyKey: "first",
      shouldPush: false,
    }),
  ).toBe(first);
  expect((await group.messages()).length).toBe(count);
  const other = await group.sendText("same", {
    idempotencyKey: "other",
    shouldPush: false,
  });
  const unkeyed = await group.sendText("same", { shouldPush: false });
  expect(new Set([first, other, unkeyed]).size).toBe(3);
  expect((await group.messages()).length).toBe(count + 2);
});
