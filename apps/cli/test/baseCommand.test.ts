import { Config } from "@oclif/core";
import { XmtpError } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

import { formatStorageErrorMessage, terminalSafeText } from "@/baseCommand";
import ConversationSync from "@/commands/conversation/sync";

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

describe("storage errors", () => {
  const storageError = (code: string, message: string) =>
    new XmtpError.Storage({
      code,
      category: "storage",
      retryable: false,
      message,
    });

  it.each([
    [
      "StorageError::PreTransitionDatabase",
      "This database predates the backend transition. Delete the database file and create a new client.",
    ],
    [
      "StorageError::OldStreamDatabase",
      "This database uses an older self-hosted format. Keep a backup and create a new client database.",
    ],
    [
      "StorageError::OldEncodingDatabase",
      "This database uses an older storage encoding. Keep a backup and create a new client database.",
    ],
  ])("prints the database path for %s", (code, sdkMessage) => {
    const { dbPath } = createTestIdentity();
    const message = formatStorageErrorMessage(
      storageError(code, sdkMessage),
      dbPath,
    );

    expect(message).toBe(`${sdkMessage}\nDatabase path: ${dbPath}`);
  });

  it("prints no path for another storage error or a message that names a code", () => {
    const { dbPath } = createTestIdentity();
    for (const error of [
      storageError("Storage", "database is locked"),
      new Error("[StorageError::OldEncodingDatabase] not an SDK error"),
    ]) {
      expect(formatStorageErrorMessage(error, dbPath)).not.toContain(
        "Database path:",
      );
    }
  });
});

describe("error output", () => {
  it("puts untrusted text on one line, with each control character escaped", () => {
    expect(terminalSafeText("\u001b]0;evil\u0007 failed\nforged\rline")).toBe(
      "\\u001b]0;evil\\u0007 failed\\u000aforged\\u000dline",
    );
    expect(
      terminalSafeText(
        "\u061c\u200b\u200e\u202e\u2060\u2066\u2069\ufeff failed",
      ),
    ).toBe("\\u061c\\u200b\\u200e\\u202e\\u2060\\u2066\\u2069\\ufeff failed");
  });

  it("escapes a plain error that reaches the command error handler", async () => {
    const command = new ConversationSync(
      ["id"],
      await Config.load({ root: process.cwd() }),
    );
    let thrown: Error | undefined;
    try {
      command.catch(new Error("bad id abc\u001b]0;evil\u0007\u202Edef"));
    } catch (error) {
      thrown = error as Error;
    }
    expect(thrown?.message).toBe("bad id abc\\u001b]0;evil\\u0007\\u202edef");
  });

  it("keeps oclif's styled parse errors readable", async () => {
    const result = await runCommand(["client", "inbox-id"], {
      env: { FORCE_COLOR: "1" },
    });

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("Missing required flag identifier");
    expect(result.stderr).not.toContain("\\u001b");
    expect(result.stderr).not.toContain("\u001b");
  });
});
