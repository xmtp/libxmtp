import { describe, expect, it } from "vitest";

import { createRegisteredIdentity, runWithIdentity } from "../../helpers.js";

describe("client revoke-installations", () => {
  it("fails without --installation-ids flag", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "client",
      "revoke-installations",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("installation-ids");
  });
});
