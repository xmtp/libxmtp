import { createRegisteredClient, createSigner } from "@test/helpers";
import { type ClientEvent, Timestamp, generateInboxId } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

describe("Dm", () => {
  it("should create a dm", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2, identifier: identifier2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    // An account identity routes to the generated `createDmWithIdentity`
    // call. Other smoke tests create DMs from inbox IDs. The options go
    // through the generated `CreateDmOptions` lowering.
    const disappearing = {
      from: new Timestamp(1n),
      retentionNs: 1_000_000_000n,
    };
    const dm = await client1.conversations.createDm(identifier2, {
      disappearing,
    });
    expect(dm).toBeDefined();
    expect(dm.id).toBeDefined();
    expect(dm.createdAt.ns).toBeDefined();
    expect(dm.createdAt).toBeDefined();
    expect((await dm.state()).isActive).toBe(true);
    expect(dm.addedByInboxId).toBe(client1.inboxId);
    expect(await dm.peerInboxId()).toBe(client2.inboxId);

    expect((await dm.messages()).length).toBe(1);

    const members = await dm.members();
    expect(members.length).toBe(2);
    const memberInboxIds = members.map((member) => member.inboxId);
    expect(memberInboxIds).toContain(client1.inboxId);
    expect(memberInboxIds).toContain(client2.inboxId);

    const metadata = {
      conversationType: dm.kind,
      creatorInboxId: dm.creatorInboxId,
    };
    expect(metadata.conversationType).toBe("dm");
    expect(metadata.creatorInboxId).toBe(client1.inboxId);

    expect((await dm.state()).consentState).toBe("allowed");

    const dms = await client1.conversations.listDms({});
    expect(dms.length).toBe(1);
    expect(dms[0].id).toBe(dm.id);

    expect((await client1.conversations.listDms({})).length).toBe(1);
    expect((await client1.conversations.listGroups({})).length).toBe(0);

    // confirm DM in other client
    await client2.conversations.sync();
    const dms2 = await client2.conversations.listDms({});
    expect(dms2.length).toBe(1);
    expect(dms2[0].id).toBe(dm.id);
    expect(await dms2[0].peerInboxId()).toBe(client1.inboxId);

    expect((await client2.conversations.listDms({})).length).toBe(1);
    expect((await client2.conversations.listGroups({})).length).toBe(0);

    const dupeDms = await dm.duplicateDms();
    expect(dupeDms.length).toEqual(0);

    // Generated lookups: an inbox ID and an identity find the DM, and a
    // missing DM lifts to undefined.
    expect(
      (await client1.conversations.getDmByInboxId(client2.inboxId))?.id,
    ).toBe(dm.id);
    expect((await client1.conversations.getDmByIdentity(identifier2))?.id).toBe(
      dm.id,
    );
    expect(
      await client1.conversations.getDmByInboxId(
        generateInboxId(createSigner().identifier),
      ),
    ).toBeUndefined();

    // The disappearing settings lift back as a Timestamp and a bigint, and an
    // expired message reaches the event payload as bytes.
    const state = await dm.state();
    expect(state.disappearingSettings).toEqual(disappearing);
    expect(state.isDisappearingEnabled).toBe(true);
    expect(state.pausedForVersion).toBeUndefined();
    const events = await client1.events({
      kinds: ["message.expired"],
      references_own_messages: false,
    });
    const seen: ClientEvent[] = [];
    const consumed = (async () => {
      for await (const event of events) seen.push(event);
    })();
    try {
      const expiringId = await dm.sendText("expires");
      await vi.waitFor(
        () => {
          const [event] = seen;
          if (event?.kind !== "message.expired")
            throw new Error("expected an expired event");
          expect(event.message_expired.message_id).toBeInstanceOf(Uint8Array);
          expect(Buffer.from(event.message_expired.message_id)).toEqual(
            Buffer.from(expiringId, "hex"),
          );
          expect(Buffer.from(event.message_expired.group_id)).toEqual(
            Buffer.from(dm.id, "hex"),
          );
        },
        { timeout: 30_000, interval: 500 },
      );
    } finally {
      await events.return();
      await consumed;
    }
  });
});
