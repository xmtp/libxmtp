import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

describe("preferences sync", () => {
  it("syncs preferences from the network", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "preferences",
      "sync",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<{
      success: boolean;
      message: string;
    }>(result.stdout);

    expect(output.success).toBe(true);
    expect(output.message).toContain("synced");
  });
});
