import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

// The SDK returns consent as string values.
const ConsentStates = {
  Unknown: "unknown",
  Allowed: "allowed",
  Denied: "denied",
} as const;

describe("conversation consent-state", () => {
  it("returns consent state for a conversation", async () => {
    const user = await createRegisteredIdentity();
    const other = await createRegisteredIdentity();

    const groupResult = await runWithIdentity(user, [
      "conversations",
      "create-group",
      other.address,
      "--json",
    ]);
    const group = parseJsonOutput<{ id: string }>(groupResult.stdout);

    const result = await runWithIdentity(user, [
      "conversation",
      "consent-state",
      group.id,
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const consent = parseJsonOutput<{ consentState: string }>(result.stdout);
    expect([
      ConsentStates.Unknown,
      ConsentStates.Allowed,
      ConsentStates.Denied,
    ]).toContain(consent.consentState);
  });
});
