import {
  createRegisteredClient,
  createUser,
  TEST_API_URL,
} from "@test/helpers";
import { generateInboxId, getInboxIdForAddress } from "../dist/index";
import { describe, expect, it } from "vitest";

describe("generateInboxId", () => {
  it("should generate an inbox id", () => {
    const user = createUser();
    const inboxId = generateInboxId(user.account.address);
    expect(inboxId).toBeDefined();
  });
});

describe("getInboxIdForAddress", () => {
  it("should return `null` inbox ID for unregistered address", async () => {
    const user = createUser();
    const inboxId = await getInboxIdForAddress(
      TEST_API_URL,
      false,
      user.account.address
    );
    expect(inboxId).toBe(null);
  });

  it("should return inbox ID for registered address", async () => {
    const user = createUser();
    const client = await createRegisteredClient(user);
    const inboxId = await getInboxIdForAddress(
      TEST_API_URL,
      false,
      user.account.address
    );
    expect(inboxId).toBe(client.inboxId());
  });
});
