import { describe, expect, it } from "vitest";
import init, { createTestClient } from "../";

await init();

describe("Error Codes", () => {
  it("should include error code in message when updating group name exceeds character limit", async () => {
    const client = await createTestClient();
    const conversation = await client.conversations().createGroupByInboxIds([]);

    // Wait for sync to complete
    await conversation.sync();

    // Try to update group name with a string that exceeds the character limit
    const longName = "a".repeat(1025);

    try {
      await conversation.updateGroupName(longName);
      expect.unreachable("Should have thrown an error");
    } catch (error: unknown) {
      const err = error as Error & { code?: string };
      // Error message should contain the error code prefix
      expect(err.message).toMatch(/^\[GroupError::/);
      // The .code property should be set
      expect(err.code).toBeDefined();
      expect(err.code).toMatch(/^GroupError::/);
    }
  });

  it("should include error code in message when adding invalid member", async () => {
    const client = await createTestClient();
    const conversation = await client.conversations().createGroupByInboxIds([]);

    await conversation.sync();

    try {
      await conversation.addMembers(["not_a_real_inbox_id"]);
      expect.unreachable("Should have thrown an error");
    } catch (error: unknown) {
      const err = error as Error & { code?: string };
      // Error message should contain an error code prefix in brackets
      expect(err.message).toMatch(/^\[/);
      // The .code property should be set
      expect(err.code).toBe("Benny-Check-This");
    }
  });
});
