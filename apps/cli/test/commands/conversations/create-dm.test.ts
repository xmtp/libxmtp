import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

interface DmResult {
  id: string;
  peerInboxId: string;
  createdAt: string;
  consentState: string;
  isActive: boolean;
  creatorInboxId: string;
  members: Array<{
    inboxId: string;
    accountIdentifiers: Array<{
      identifier: string;
      kind: string;
    }>;
    permissionLevel: string;
  }>;
}

describe("conversations create-dm", () => {
  it("returns same DM when created twice", async () => {
    const sender = await createRegisteredIdentity();
    const recipient = await createRegisteredIdentity();

    const result1 = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--json",
    ]);
    const result2 = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--json",
    ]);
    const upperCaseResult = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address.toUpperCase(),
      "--json",
    ]);
    const explicitKindResult = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--identifier-kind",
      "ethereum",
      "--json",
    ]);

    expect(result1.exitCode).toBe(0);
    expect(result2.exitCode).toBe(0);
    expect(upperCaseResult.exitCode).toBe(0);
    expect(explicitKindResult.exitCode).toBe(0);

    const output1 = parseJsonOutput<DmResult>(result1.stdout);
    const output2 = parseJsonOutput<DmResult>(result2.stdout);
    const upperCaseOutput = parseJsonOutput<DmResult>(upperCaseResult.stdout);
    const explicitKindOutput = parseJsonOutput<DmResult>(
      explicitKindResult.stdout,
    );

    expect(output1.id).toBeDefined();
    expect(output1.peerInboxId).toBe(recipient.inboxId);
    expect(output1.createdAt).toBeDefined();
    expect(output1.isActive).toBe(true);
    expect(output1.members).toHaveLength(2);
    expect(output1.id).toBe(output2.id);
    expect(output1.id).toBe(upperCaseOutput.id);
    expect(output1.id).toBe(explicitKindOutput.id);
  });

  it("fails without recipient identifier", async () => {
    const sender = await createRegisteredIdentity();

    const result = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
  });

  it("both parties can see the DM", async () => {
    const sender = await createRegisteredIdentity();
    const recipient = await createRegisteredIdentity();

    // Create DM from sender's perspective
    const createResult = await runWithIdentity(sender, [
      "conversations",
      "create-dm",
      recipient.address,
      "--json",
    ]);
    expect(createResult.exitCode).toBe(0);

    // Sync recipient's conversations
    await runWithIdentity(recipient, ["conversations", "sync"]);

    // List recipient's DMs
    const listResult = await runWithIdentity(recipient, [
      "conversations",
      "list",
      "--type",
      "dm",
      "--json",
    ]);
    expect(listResult.exitCode).toBe(0);

    const dms = parseJsonOutput<DmResult[]>(listResult.stdout);
    const createOutput = parseJsonOutput<DmResult>(createResult.stdout);

    expect(dms.some((dm) => dm.id === createOutput.id)).toBe(true);
  });
});
