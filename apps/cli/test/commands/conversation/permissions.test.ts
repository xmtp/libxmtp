import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

describe("conversation permissions", () => {
  it("reads the policy set by each group permission flag", async () => {
    const creator = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();

    for (const [flag, policyType] of [
      ["admin-only", "adminOnly"],
      ["all-members", "allMembers"],
    ] as const) {
      const groupResult = await runWithIdentity(creator, [
        "conversations",
        "create-group",
        member.address,
        "--permissions",
        flag,
        "--json",
      ]);
      expect(groupResult.exitCode).toBe(0);
      const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

      const result = await runWithIdentity(creator, [
        "conversation",
        "permissions",
        group.id,
        "--json",
      ]);
      expect(result.exitCode).toBe(0);

      const output = parseJsonOutput<{
        conversationId: string;
        permissions: { policyType: string; policySet: unknown };
      }>(result.stdout);
      expect(output.conversationId).toBe(group.id);
      expect(output.permissions.policyType).toBe(policyType);
      expect(output.permissions.policySet).toBeDefined();
    }
  });

  it("fails for DM conversation", async () => {
    const sender = await createRegisteredIdentity();
    const recipient = await createRegisteredIdentity();

    const dmResult = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--json",
    ]);
    const dm = parseJsonOutput<{ id: string }>(dmResult.stdout);

    const result = await runWithIdentity(sender, [
      "conversation",
      "permissions",
      dm.id,
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("group");
  });
});
