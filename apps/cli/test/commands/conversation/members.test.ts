import { describe, expect, it } from "vitest";

import {
  createIdentityWithHandle,
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface Member {
  inboxId: string;
  handle: string | null;
  otherHandles: string[];
  installationIds: string[];
  accountIdentifiers: Array<{
    identifier: string;
    kind: string;
  }>;
  permissionLevel: string;
}

describe("conversation members", () => {
  it("lists members of a group", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createRegisteredIdentity();

    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      member2.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);

    expect(membersResult.exitCode).toBe(0);

    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(3); // Creator + 2 members
    expect(members.map((m) => m.inboxId)).toContain(creator.inboxId);
    expect(members.map((m) => m.inboxId)).toContain(member1.inboxId);
    expect(members.map((m) => m.inboxId)).toContain(member2.inboxId);
    for (const identity of [creator, member1, member2]) {
      expect(
        members.find((member) => member.inboxId === identity.inboxId)
          ?.installationIds,
      ).toContain(identity.installationId);
    }
  });

  it("lists members of a DM", async () => {
    const sender = await createRegisteredIdentity();
    const recipient = await createRegisteredIdentity();

    const dmResult = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--json",
    ]);
    const dm = parseJsonOutput<{ id: string }>(dmResult.stdout);

    const membersResult = await runWithIdentity(sender, [
      "conversation",
      "members",
      dm.id,
      "--json",
    ]);

    expect(membersResult.exitCode).toBe(0);

    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(2);
    for (const identity of [sender, recipient]) {
      expect(
        members.find((member) => member.inboxId === identity.inboxId)
          ?.installationIds,
      ).toContain(identity.installationId);
    }
  });

  it("shows each member's current handle", async () => {
    const creator = await createRegisteredIdentity();
    const member = await createIdentityWithHandle();

    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member.address,
      "--json",
    ]);
    expect(groupResult.exitCode).toBe(0);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const json = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    expect(json.exitCode).toBe(0);
    const members = parseJsonOutput<Member[]>(json.stdout);
    const handles = (inboxId: string) => {
      const found = members.find((entry) => entry.inboxId === inboxId);
      return { handle: found?.handle, otherHandles: found?.otherHandles };
    };
    expect(handles(member.inboxId)).toEqual({
      handle: member.handle,
      otherHandles: [],
    });
    expect(handles(creator.inboxId)).toEqual({
      handle: null,
      otherHandles: [],
    });

    const human = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
    ]);
    expect(human.exitCode).toBe(0);
    const [header, , ...rows] = human.stdout.split("\n");
    expect(header.split(/\s+/)).toContain("handle");
    expect(header).not.toContain("otherHandles");
    expect(
      rows.find((row) => row.startsWith(member.inboxId))?.split(/\s+/)[1],
    ).toBe(member.handle);
  });
});
