import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface GroupResult {
  id: string;
  name: string | null;
  description: string | null;
  imageUrl: string | null;
  createdAt: string;
  memberCount: number;
  members: Array<{
    inboxId: string;
    accountIdentifiers: Array<{
      identifier: string;
      kind: string;
    }>;
    permissionLevel: string;
  }>;
}

describe("conversations create-group", () => {
  it("creates a group with multiple members", async () => {
    const creator = await createRegisteredIdentity();
    const member1 = await createRegisteredIdentity();
    const member2 = await createRegisteredIdentity();

    const result = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member1.address,
      member2.address,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<GroupResult>(result.stdout);
    expect(output.memberCount).toBe(3); // Creator + 2 members
  });

  it("creates a group with all metadata", async () => {
    const creator = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();

    const result = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member.address,
      "--name",
      "Full Metadata Group",
      "--description",
      "A group with all metadata",
      "--image-url",
      "https://example.com/group.png",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<GroupResult>(result.stdout);
    expect(output.id).toBeDefined();
    expect(output.memberCount).toBe(2);
    expect(output.createdAt).toBeDefined();
    expect(output.name).toBe("Full Metadata Group");
    expect(output.description).toBe("A group with all metadata");
    expect(output.imageUrl).toBe("https://example.com/group.png");
  });

  it("fails without any members", async () => {
    const creator = await createRegisteredIdentity();

    const result = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
  });

  it("handles case-insensitive addresses", async () => {
    const creator = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();
    const upperCaseAddress = member.address.toUpperCase();

    const result = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      upperCaseAddress,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<GroupResult>(result.stdout);
    expect(output.memberCount).toBe(2);
  });
});
