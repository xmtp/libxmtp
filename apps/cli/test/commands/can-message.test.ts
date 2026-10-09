import { describe, expect, it } from "vitest";

import {
  backendFlags,
  createRegisteredIdentity,
  parseJsonOutput,
  runCommand,
} from "../helpers.js";

interface CanMessageResult {
  identifier: string;
  reachable: boolean;
}

describe("can-message", () => {
  it("handles mixed registered and unregistered addresses", async () => {
    const first = await createRegisteredIdentity();
    const second = await createRegisteredIdentity();
    const upperCaseAddress = first.address.toUpperCase();
    const unregisteredAddress = "0x0000000000000000000000000000000000000002";

    const result = await runCommand(
      [
        "can-message",
        upperCaseAddress,
        second.address,
        unregisteredAddress,
        ...backendFlags(),
        "--json",
      ],
      { env: { XMTP_BACKEND_URL: "not-a-url" } },
    );

    expect(result.exitCode).toBe(0);

    const output = parseJsonOutput<CanMessageResult[]>(result.stdout);
    expect(output).toEqual([
      { identifier: upperCaseAddress, reachable: true },
      { identifier: second.address, reachable: true },
      { identifier: unregisteredAddress, reachable: false },
    ]);

    const human = await runCommand([
      "can-message",
      first.address,
      ...backendFlags(),
    ]);
    expect(human.exitCode).toBe(0);
    expect(human.stdout.toLowerCase()).toContain(first.address.toLowerCase());
  });

  it("requires at least one address", async () => {
    const result = await runCommand(["can-message", ...backendFlags()]);

    expect(result.exitCode).not.toBe(0);
  });
});
