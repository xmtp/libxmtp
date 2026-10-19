import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../helpers.js";

describe("address-authorized", () => {
  it("returns true for authorized address", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "address-authorized",
      identity.inboxId,
      identity.address,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<{
      inboxId: string;
      address: string;
      isAuthorized: boolean;
    }>(result.stdout);

    expect(output.inboxId).toBe(identity.inboxId);
    expect(output.address.toLowerCase()).toBe(identity.address.toLowerCase());
    expect(output.isAuthorized).toBe(true);
  });

  it("fails without required arguments", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "address-authorized",
      identity.inboxId,
      // Missing address argument
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
  });
});
