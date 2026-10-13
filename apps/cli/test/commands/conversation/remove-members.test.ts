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

describe("conversation remove-members", () => {
  it("removes a member from a group", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createRegisteredIdentity();

    // Create group with two members
    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      member2.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    // Remove one member by address
    const removeResult = await runWithIdentity(creator, [
      "conversation",
      "remove-members",
      group.id,
      member2.address,
      "--json",
    ]);

    expect(removeResult.exitCode).toBe(0);

    // Verify member was removed
    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(2);
    expect(members.map((m) => m.inboxId)).not.toContain(member2.inboxId);
  });

  it("removes a member by handle", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createIdentityWithHandle();

    const groupResult = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      member2.address,
      "--json",
    ]);
    expect(groupResult.exitCode).toBe(0);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const removeResult = await runWithIdentity(creator, [
      "conversation",
      "remove-members",
      group.id,
      member2.handle,
      "--identifier-kind",
      "handle",
      "--json",
    ]);

    expect(removeResult.exitCode).toBe(0);

    const membersResult = await runWithIdentity(creator, [
      "conversation",
      "members",
      group.id,
      "--json",
    ]);
    const members = parseJsonOutput<Member[]>(membersResult.stdout);
    expect(members.length).toBe(2);
    expect(members.map((m) => m.inboxId)).not.toContain(member2.inboxId);
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

    const removeResult = await runWithIdentity(creator, [
      "conversation",
      "remove-members",
      group.id,
      handle,
      "--identifier-kind",
      "handle",
    ]);

    expect(removeResult.exitCode).not.toBe(0);
    expect(removeResult.stderr.split("\n")).toContain(
      `No inbox has the handle ${handle}.`,
    );
    expect(removeResult.stderr).not.toContain("USAGE");
  });
});
