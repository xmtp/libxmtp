import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface Conversation {
  id: string;
  createdAt: string;
  createdAtNs: string;
  isActive: boolean;
}

describe("conversations list", () => {
  it("lists all conversations", async () => {
    const user = await createRegisteredIdentity();
    const other1 = await createRegisteredIdentity();
    const other2 = await createRegisteredIdentity();

    // Create a group and a DM
    await runWithIdentity(user, [
      "conversations",
      "create-group",
      other1.address,
    ]);
    await runWithIdentity(user, ["conversations", "create-dm", other2.address]);

    const result = await runWithIdentity(user, [
      "conversations",
      "list",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const conversations = parseJsonOutput<Conversation[]>(result.stdout);
    expect(conversations.length).toBe(2);
  });

  it("pages after a conversation with --before-id", async () => {
    const user = await createRegisteredIdentity();
    const other1 = await createRegisteredIdentity();
    const other2 = await createRegisteredIdentity();
    await runWithIdentity(user, ["conversations", "create-dm", other1.address]);
    await runWithIdentity(user, ["conversations", "create-dm", other2.address]);

    const page = async (...extra: string[]) => {
      const result = await runWithIdentity(user, [
        "conversations",
        "list",
        "--order-by",
        "created-at",
        "--limit",
        "1",
        "--json",
        ...extra,
      ]);
      expect(result.exitCode).toBe(0);
      return parseJsonOutput<Conversation[]>(result.stdout);
    };
    const [first] = await page();
    const [second] = await page(
      "--created-before",
      first!.createdAtNs,
      "--before-id",
      first!.id,
    );
    expect(second).toBeDefined();
    expect(second!.id).not.toBe(first!.id);
    expect(BigInt(second!.createdAtNs)).toBeLessThanOrEqual(
      BigInt(first!.createdAtNs),
    );

    // The cursor reaches the SDK: a before id needs the before time.
    const withoutTime = await runWithIdentity(user, [
      "conversations",
      "list",
      "--order-by",
      "created-at",
      "--before-id",
      first!.id,
    ]);
    expect(withoutTime.exitCode).not.toBe(0);
  });

  it("fails with invalid created-after value", async () => {
    const user = await createRegisteredIdentity();

    const result = await runWithIdentity(user, [
      "conversations",
      "list",
      "--created-after",
      "not-a-number",
    ]);

    expect(result.exitCode).toBe(2);
    expect(result.stderr).toContain("Invalid value for --created-after");
  });
});
