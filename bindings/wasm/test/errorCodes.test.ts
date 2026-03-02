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
      expect(
        err.message,
        "Error message should contain the error code prefix",
      ).toBe(
        "[GroupError::TooManyCharacters] Exceeded max characters for this field. Must be under: 100",
      );
      expect(err.code, 'The "code" property should be set').toBe(
        "GroupError::TooManyCharacters",
      );
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
      expect(
        err.message.startsWith(
          "[GroupError::Client] client: API error: api client error api client at endpoint",
        ),
        'Error message should contain an error "code" prefix in brackets',
      ).toBe(true);
      expect(err.code, 'The "code" property should be set').toBe(
        "GroupError::Client",
      );
    }
  });
});
