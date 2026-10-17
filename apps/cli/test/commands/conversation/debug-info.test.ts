import { describe, expect, it } from "vitest";

import { createRegisteredIdentity, runWithIdentity } from "../../helpers.js";

describe("conversation debug-info", () => {
  it("fails for non-existent conversation", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "conversation",
      "debug-info",
      "non-existent-id",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
  });
});
