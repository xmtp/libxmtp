import { describe, expect, it } from "vitest";

import {
  claimHandle,
  createTestIdentity,
  parseJsonOutput,
  randomHandleName,
  runWithIdentity,
} from "../../helpers.js";

interface ClientInfoOutput {
  properties: {
    address: string;
    inboxId: string;
    handle: string | null;
    otherHandles: string[];
    installationId: string;
    isRegistered: boolean;
    appVersion: string;
    libxmtpVersion: string;
  };
  options: {
    env: string;
    backendUrl: string;
    dbPath: string;
    loggingLevel?: string;
    structuredLogging?: boolean;
    disableAutoRegister?: boolean;
    disableDeviceSync?: boolean;
    appVersion?: string;
    nonce?: string;
  };
}

describe("client info", () => {
  it("returns client info with JSON output", async () => {
    const identity = createTestIdentity();
    const result = await runWithIdentity(identity, [
      "client",
      "info",
      "--json",
    ]);

    expect(result.exitCode).toBe(0);

    const info = parseJsonOutput<ClientInfoOutput>(result.stdout);
    expect(info.properties.address).toMatch(/^0x[a-f0-9]{40}$/i);
    expect(info.properties.inboxId).toBeDefined();
    expect(info.properties.installationId).toBeDefined();
    expect(info.properties.isRegistered).toBe(true);
    expect(info.properties.libxmtpVersion).toBeDefined();
    expect(info.properties.appVersion).toBeDefined();
    expect(info.options.appVersion).toMatch(/^xmtp-cli\/\d+\.\d+\.\d+/);
    expect(info.options.env).toBe("local");
    expect(info.options.backendUrl).toBe(process.env.XMTP_BACKEND_URL);
    expect(info.options).not.toHaveProperty("apiUrl");
    expect(info.options).not.toHaveProperty("historySyncUrl");
    expect(info.options).not.toHaveProperty("gatewayHost");
    expect(info.options).not.toHaveProperty("storage.encryptionKey");
    expect(info.options.dbPath).toBeDefined();
  });

  it("outputs table format by default", async () => {
    const identity = createTestIdentity();
    const result = await runWithIdentity(identity, ["client", "info"]);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("Client");
    expect(result.stdout).toContain("address");
    expect(result.stdout).toContain("inboxId");
    expect(result.stdout).toContain("Options");
    expect(result.stdout).toContain("env");
    expect(result.stdout).not.toContain("encryptionKey");
  });

  it("fails without wallet key", async () => {
    const identity = createTestIdentity();
    const result = await runWithIdentity({ ...identity, walletKey: "" }, [
      "client",
      "info",
    ]);

    expect(result.exitCode).not.toBe(0);
  });

  it("shows the current handle after a claim", async () => {
    const identity = createTestIdentity();
    const before = await runWithIdentity(identity, [
      "client",
      "info",
      "--json",
    ]);
    expect(before.exitCode).toBe(0);
    const unclaimed = parseJsonOutput<ClientInfoOutput>(before.stdout);
    expect(unclaimed.properties.handle).toBeNull();
    expect(unclaimed.properties.otherHandles).toEqual([]);

    const handle = randomHandleName();
    await claimHandle(identity, handle);

    const json = await runWithIdentity(identity, ["client", "info", "--json"]);
    expect(json.exitCode).toBe(0);
    const claimed = parseJsonOutput<ClientInfoOutput>(json.stdout);
    expect(claimed.properties.handle).toBe(handle);
    expect(claimed.properties.otherHandles).toEqual([]);

    const human = await runWithIdentity(identity, ["client", "info"]);
    expect(human.exitCode).toBe(0);
    expect(human.stdout).toMatch(new RegExp(`^\\s+handle\\s+${handle}$`, "m"));
  });
});
