import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "../../helpers.js";

describe("client change-recovery-identifier", () => {
  it("changes the recovery identifier only to an added account", async () => {
    const identity = await createRegisteredIdentity();
    const newRecoveryKey = generatePrivateKey();
    const newRecoveryAddress = privateKeyToAccount(newRecoveryKey).address;
    const changeRecovery = () =>
      runWithIdentity(identity, [
        "client",
        "change-recovery-identifier",
        "--identifier",
        newRecoveryAddress,
        "--force",
        "--json",
      ]);

    // The new recovery identifier must already be an account of the inbox.
    const refused = await changeRecovery();
    expect(refused.exitCode).not.toBe(0);
    expect(refused.stderr).toContain(
      "the new recovery identity is not an identity of the inbox",
    );

    const added = await runWithIdentity(identity, [
      "client",
      "add-account",
      "--new-wallet-key",
      newRecoveryKey,
      "--force",
      "--json",
    ]);
    expect(added.exitCode).toBe(0);

    const result = await changeRecovery();
    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<{
      success: boolean;
      newRecoveryIdentifier: string;
      kind: string;
      inboxId: string;
    }>(result.stdout);

    expect(output.success).toBe(true);
    expect(output.newRecoveryIdentifier.toLowerCase()).toBe(
      newRecoveryAddress.toLowerCase(),
    );
    expect(output.kind).toBe("ethereum");

    const state = await runWithIdentity(identity, [
      "preferences",
      "inbox-state",
      "--sync",
      "--json",
    ]);
    expect(state.exitCode).toBe(0);
    const { recoveryIdentifier } = parseJsonOutput<{
      recoveryIdentifier: { identifier: string };
    }>(state.stdout);
    expect(recoveryIdentifier.identifier.toLowerCase()).toBe(
      newRecoveryAddress.toLowerCase(),
    );
  });

  it("fails without --identifier flag", async () => {
    const identity = await createRegisteredIdentity();

    const result = await runWithIdentity(identity, [
      "client",
      "change-recovery-identifier",
      "--json",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("identifier");
  });
});
