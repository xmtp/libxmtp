import { describe, expect, it } from "vitest";
import { createTestIdentity, runCommand } from "./helpers.js";

function identityFlags() {
  const identity = createTestIdentity();
  return [
    "--wallet-key",
    identity.walletKey,
    "--db-encryption-key",
    identity.dbEncryptionKey,
    "--db-path",
    identity.dbPath,
  ];
}

describe("network configuration", () => {
  it("requires a backend URL", async () => {
    const result = await runCommand(["client", "info", ...identityFlags()], {
      env: { XMTP_BACKEND_URL: "" },
    });

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain(
      "Backend URL is required. Set XMTP_BACKEND_URL, use --backend-url, or run 'xmtp init --backend-url <url>'.",
    );
  });

  it.each(["http://", "https://[", "ftp://example.com"])(
    "rejects invalid backend URL %s",
    async (backendUrl) => {
      const result = await runCommand([
        "client",
        "info",
        ...identityFlags(),
        "--backend-url",
        backendUrl,
      ]);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain(
        "Backend URL must be a valid http:// or https:// URL.",
      );
    },
  );

  it.each(["", ".", "..", "a/b", "a\\b"])(
    "rejects invalid database label %s",
    async (label) => {
      const result = await runCommand([
        "client",
        "info",
        ...identityFlags(),
        "--backend-url",
        "http://127.0.0.1:5050",
        `--env=${label}`,
      ]);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Environment label must be non-empty");
    },
  );
});
