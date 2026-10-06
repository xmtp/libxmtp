import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

describe("Dm", () => {
  it("should create a dm", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2, identifier: identifier2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);

    // An account identity routes to the generated `createDmWithIdentity`
    // call. Other smoke tests create DMs from inbox IDs.
    const dm = await client1.conversations.createDm(identifier2);
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
  });
});
