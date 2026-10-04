import {
  Client,
  ConversationStream,
  MessageStream,
  Timestamp,
  type ClientEvent,
} from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test, vi } from "vitest";

import { backend, create, signer } from "./helpers";

beforeAll(() => initPureWasm());

test("consent records keep entity types and listener values with an omitted own-message filter", async () => {
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
  } finally {
    await client.stopListener(listener);
  }
});

test("independent DMs stitch messages and duplicate groups", async () => {
  const client = await create();
  const peer = await create();
  const first = await client.conversations.createDm(peer.inboxId);
  const second = await peer.conversations.createDm(client.inboxId);
  expect(first.id).not.toBe(second.id);
  const firstId = await first.sendText("first topic", { shouldPush: true });
  const secondId = await second.sendText("second topic", { shouldPush: true });
  await client.conversations.sync();
  await peer.conversations.sync();
  await first.sync();
  await second.sync();
  const stitched = await client.conversations.getDmByInboxId(peer.inboxId);
  expect(stitched?.id).toBe(second.id);
  expect((await stitched?.duplicateDms())?.map((dm) => dm.id)).toEqual([
    first.id,
  ]);
  expect((await stitched?.messages())?.map((message) => message.id)).toEqual(
    expect.arrayContaining([firstId, secondId]),
  );
  expect(
    (await client.conversations.listDms(undefined)).map((dm) => dm.id),
  ).toEqual([second.id]);
});

test.each(["dm", "group"] as const)(
  "message reader %s filter excludes the other conversation kind",
  async (kind) => {
    const client = await create();
    const peer = await create();
    const group = await client.conversations.createGroup([]);
    const dm = await client.conversations.createDm(peer.inboxId);
    const ids: string[] = [];
    const stream = MessageStream.open(client, { conversationKind: kind });
    await stream.ready();
    const read = stream.onValue((message) => {
      ids.push(message.id);
    });
    try {
      const groupId = await group.sendText("group", { shouldPush: true });
      const dmId = await dm.sendText("dm", { shouldPush: true });
      const included = kind === "dm" ? dmId : groupId;
      const excluded = kind === "dm" ? groupId : dmId;
      await vi.waitFor(() => expect(ids).toContain(included), {
        timeout: 30_000,
      });
      expect(ids).not.toContain(excluded);
    } finally {
      await stream.end();
      await read;
    }
  },
);

test("conversation readers keep group and DM filters separate", async () => {
  const client = await create();
  const peer = await create();
  const groups: string[] = [];
  const dms: string[] = [];
  const groupStream = ConversationStream.open(client, { kind: "group" });
  const dmStream = ConversationStream.open(client, { kind: "dm" });
  await groupStream.ready();
  await dmStream.ready();
  const groupRead = groupStream.onValue((value) => {
    groups.push(value.id);
  });
  const dmRead = dmStream.onValue((value) => {
    dms.push(value.id);
  });
  try {
    const group = await client.conversations.createGroup([peer.identity]);
    const dm = await client.conversations.createDm(peer.inboxId);
    await vi.waitFor(
      () => {
        expect(groups).toEqual([group.id]);
        expect(dms).toEqual([dm.id]);
      },
      { timeout: 30_000 },
    );
  } finally {
    await groupStream.end();
    await dmStream.end();
    await groupRead;
    await dmRead;
  }
});

test("default, admin-only, and custom permission sets keep all policies", async () => {
  const client = await create();
  const defaults = {
    addMember: "allow",
    removeMember: "admin",
    addAdmin: "superAdmin",
    removeAdmin: "superAdmin",
    updateName: "allow",
    updateDescription: "allow",
    updateImage: "allow",
    updateDisappearing: "admin",
    updateAppData: "allow",
  } as const;
  const admins = {
    ...defaults,
    addMember: "admin",
    updateName: "admin",
    updateDescription: "admin",
    updateImage: "admin",
    updateAppData: "admin",
  } as const;
  const custom = {
    ...admins,
    addAdmin: "deny",
    removeAdmin: "deny",
    updateName: "deny",
  } as const;
  for (const [permissions, policySet, policyType] of [
    [{ kind: "allMembers" }, defaults, "allMembers"],
    [{ kind: "adminOnly" }, admins, "adminOnly"],
    [{ kind: "custom", policySet: custom }, custom, "custom"],
  ] as const) {
    const group = await client.conversations.createGroup([], { permissions });
    expect((await group.state()).permissions).toEqual({
      policySet,
      policyType,
    });
  }
});

test.each(["group", "dm"] as const)(
  "%s message filters keep content, sender, timestamps, delivery status, and order",
  async (kind) => {
    const client = await create();
    const peer = await create();
    const group =
      kind === "group"
        ? await client.conversations.createGroup([])
        : await client.conversations.createDm(peer.inboxId);
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
    expect(await group.messages({ contentTypes: [codec.type] })).toHaveLength(
      2,
    );
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
    expect(
      await group.messages({ sentBefore: new Timestamp(1n) }),
    ).toHaveLength(0);
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
  },
);

test("a new group and DM sync to registered peer installations", async () => {
  const client = await create();
  const owner = signer();
  const first = await create(owner);
  const second = await create(owner);
  const group = await client.conversations.createGroup([first.inboxId]);
  const dm = await client.conversations.createDm(first.inboxId);
  for (const peer of [first, second]) {
    await peer.conversations.sync();
    expect(
      (await peer.conversations.listGroups(undefined)).map((item) => item.id),
    ).toEqual([group.id]);
    expect(
      (await peer.conversations.list(undefined)).map((item) => item.id),
    ).toEqual(expect.arrayContaining([group.id, dm.id]));
  }
});
