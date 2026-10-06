import { Client, Timestamp, type ClientEvent } from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";
import { hexToBytes } from "viem";
import { beforeAll, expect, test, vi } from "vitest";

import { backend, create } from "./helpers";

beforeAll(() => initPureWasm());

// verifies: EVENT-007
// verifies: EVENT-024
test("listeners deliver consent records with an omitted own-message filter and exact local deletions", async () => {
  const client = await create();
  const peer = await create();
  const group = await client.conversations.createGroup([peer.inboxId]);
  const conversation = {
    kind: "conversation",
    conversationId: group.id,
  } as const;
  const inbox = { kind: "inbox", inboxId: peer.inboxId } as const;
  const changes: ClientEvent[] = [];
  const listener = await client.startListener(
    { kinds: ["consent.changed"] },
    (event) => {
      changes.push(event);
    },
  );
  const deletions: ClientEvent[] = [];
  const deletionListener = await client.startListener(
    { kinds: ["message.deleted"], references_own_messages: true },
    (event) => {
      deletions.push(event);
    },
  );
  try {
    await client.preferences.setConsentStates([
      { entity: conversation, state: "denied" },
      { entity: inbox, state: "allowed" },
    ]);
    expect(await client.preferences.consentState(conversation)).toBe("denied");
    expect(await client.preferences.consentState(inbox)).toBe("allowed");
    await vi.waitFor(
      () => {
        expect(changes).toContainEqual(
          expect.objectContaining({
            kind: "consent.changed",
            consent_changed: {
              entity_kind: "conversation",
              entity: group.id,
              state: "denied",
            },
          }),
        );
        expect(changes).toContainEqual(
          expect.objectContaining({
            kind: "consent.changed",
            consent_changed: {
              entity_kind: "inbox",
              entity: peer.inboxId,
              state: "allowed",
            },
          }),
        );
      },
      { timeout: 30_000 },
    );
    const states = await Client.inboxStates(
      [peer.inboxId, client.inboxId],
      backend,
    );
    expect(states.map((state) => state.inboxId)).toEqual([
      peer.inboxId,
      client.inboxId,
    ]);

    const id = await group.sendText("delete locally", { shouldPush: false });
    expect((await client.conversations.getMessageById(id))?.senderInboxId).toBe(
      client.inboxId,
    );
    expect((await group.messages()).map((message) => message.id)).toContain(id);
    await client.conversations.deleteMessageLocally(id);
    await vi.waitFor(() => expect(deletions).toHaveLength(1), {
      timeout: 10_000,
    });
    expect(await client.conversations.getMessageById(id)).toBeUndefined();
    expect((await group.messages()).map((message) => message.id)).not.toContain(
      id,
    );
    // The reads above give a duplicate event time to arrive.
    expect(deletions).toEqual([
      {
        kind: "message.deleted",
        message_deleted: {
          group_id: hexToBytes(`0x${group.id}`),
          message_id: hexToBytes(`0x${id}`),
          cause: "deleted_locally",
        },
      },
    ]);
  } finally {
    await client.stopListener(deletionListener);
    await client.stopListener(listener);
  }
});

test("message filters keep content, sender, timestamps, delivery status, and order", async () => {
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const codec = new TextCodec();
  const first = await group.sendText("first", { shouldPush: true });
  const second = await group.sendText("second", { shouldPush: true });
  const values = await group.messages({
    kind: "application",
    direction: "ascending",
    sortBy: "sentAt",
  });
  expect(values.map((message) => message.id)).toEqual([first, second]);
  expect(
    (
      await group.messages({
        kind: "application",
        direction: "descending",
        limit: 1,
      })
    )[0]?.id,
  ).toBe(second);
  expect(await group.messages({ contentTypes: [codec.type] })).toHaveLength(2);
  expect(
    await group.messages({ excludeContentTypes: [codec.type] }),
  ).not.toEqual(expect.arrayContaining(values));
  expect(
    await group.messages({ excludeSenderInboxIds: [client.inboxId] }),
  ).toHaveLength(0);
  expect(
    await group.messages({
      kind: "application",
      deliveryStatus: "published",
    }),
  ).toHaveLength(2);
  expect(await group.messages({ sentBefore: new Timestamp(1n) })).toHaveLength(
    0,
  );
  expect(
    await group.messages({ insertedAfter: new Timestamp(2n ** 63n - 1n) }),
  ).toHaveLength(0);
  expect(
    await group.countMessages({
      kind: "application",
      contentTypes: [codec.type],
    }),
  ).toBe(2n);
  expect(
    await group.countMessages({
      kind: "application",
      excludeContentTypes: [codec.type],
    }),
  ).toBe(0n);
});
