import { describe, expect, it } from "vitest";

import { installationIdsFromFlags } from "../../../src/commands/client/key-package-status.js";
import { createRegisteredIdentity, runWithIdentity } from "../../helpers.js";

describe("client key-package-status", () => {
  it("normalizes prefixed and uppercase installation IDs", () => {
    const upper = "AB".repeat(32);
    const lower = "cd".repeat(32);
    expect(installationIdsFromFlags([` 0x${upper}, ${lower} `])).toEqual([
      upper.toLowerCase(),
      lower,
    ]);
  });

  it("fails without --installation-ids flag", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "client",
      "key-package-status",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("installation-ids");
  });
});
