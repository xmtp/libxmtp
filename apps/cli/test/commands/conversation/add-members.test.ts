import { describe, expect, it } from "vitest";

import {
  createIdentityWithHandle,
  createRegisteredIdentity,
  parseJsonOutput,
  randomHandleName,
  runWithIdentity,
} from "../../helpers.js";

interface Member {
  inboxId: string;
  accountIdentifiers: Array<{
    identifier: string;
    kind: string;
  }>;
  permissionLevel: string;
}

describe("conversation add-members", () => {
  it("adds a member to a group", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createRegisteredIdentity();

    // Create group with one member
    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    // Add another member by address
    const addResult = await runWithIdentity(creator, [
      "conversation",
      "add-members",
      group.id,
      member2.address,
      "--json",
    ]);

    expect(addResult.exitCode).toBe(0);

    // Verify member was added
    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(3);
    expect(members.map((m) => m.inboxId)).toContain(member2.inboxId);
  });

  it("adds multiple members at once", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createRegisteredIdentity();
    const member3 = await createRegisteredIdentity();

    // Create group with one member
    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    // Add two more members by address
    const addResult = await runWithIdentity(creator, [
      "conversation",
      "add-members",
      group.id,
      member2.address,
      member3.address,
      "--json",
    ]);

    expect(addResult.exitCode).toBe(0);

    // Verify members were added
    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(4);
  });

  it("adds a member by handle", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createIdentityWithHandle();

    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      "--json",
    ]);
    expect(groupResult.exitCode).toBe(0);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const addResult = await runWithIdentity(creator, [
      "conversation",
      "add-members",
      group.id,
      member2.handle,
      "--identifier-kind",
      "handle",
      "--json",
    ]);

    expect(addResult.exitCode).toBe(0);

    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(3);
    expect(members.map((m) => m.inboxId)).toContain(member2.inboxId);
  });

  it("names a handle that no inbox holds", async () => {
    const creator = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();
    const handle = randomHandleName();

    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member.address,
      "--json",
    ]);
    expect(groupResult.exitCode).toBe(0);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const addResult = await runWithIdentity(creator, [
      "conversation",
      "add-members",
      group.id,
      handle,
      "--identifier-kind",
      "handle",
    ]);

    expect(addResult.exitCode).not.toBe(0);
    expect(addResult.stderr.split("\n")).toContain(
      `No inbox has the handle ${handle}.`,
    );
    expect(addResult.stderr).not.toContain("USAGE");
  });
});
