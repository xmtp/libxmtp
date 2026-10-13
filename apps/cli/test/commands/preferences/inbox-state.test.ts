import { describe, expect, it } from "vitest";

import {
  createIdentityWithHandle,
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface InboxState {
  inboxId: string;
  handle: string | null;
  otherHandles: string[];
  recoveryIdentifier: {
    identifier: string;
    kind: string;
  };
  identifiers: Array<{
    identifier: string;
    kind: string;
  }>;
  installations: Array<{
    id: string;
    createdAtNs: string;
  }>;
}

describe("preferences inbox-state", () => {
  it("returns the current inbox state", async () => {
    const user = await createRegisteredIdentity();

    const result = await runWithIdentity(user, [
      "preferences",
      "inbox-state",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const state = parseJsonOutput<InboxState>(result.stdout);
    expect(state.inboxId).toBe(user.inboxId);
    expect(state.recoveryIdentifier).toBeDefined();
    expect(state.identifiers.length).toBeGreaterThan(0);
    expect(state.installations.length).toBeGreaterThan(0);
  });

  it("fetches fresh inbox state with --sync flag", async () => {
    const user = await createRegisteredIdentity();

    const result = await runWithIdentity(user, [
      "preferences",
      "inbox-state",
      "--sync",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const state = parseJsonOutput<InboxState>(result.stdout);
    expect(state.inboxId).toBe(user.inboxId);
  });

  it("shows the handles of this inbox", async () => {
    const user = await createIdentityWithHandle();

    const result = await runWithIdentity(user, [
      "preferences",
      "inbox-state",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);
    const state = parseJsonOutput<InboxState>(result.stdout);
    expect(state.handle).toBe(user.handle);
    expect(state.otherHandles).toEqual([]);
  });
});
