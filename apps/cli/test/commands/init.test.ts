import { chmod, open, readFile, stat } from "node:fs/promises";
import { dirname, join } from "node:path";

import { describe, expect, it } from "vitest";

import { getTestEnvPath, runCommand } from "../helpers.js";

const backendUrl = "http://127.0.0.1:5050";
const initFlags = ["--backend-url", backendUrl];

describe("init", () => {
  it("requires a backend URL without writing a file", async () => {
    const testPath = getTestEnvPath();
    const result = await runCommand(["init", "--output", testPath]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("Missing required flag backend-url");
    await expect(readFile(testPath, "utf-8")).rejects.toThrow();
  });

  it("generates keys and writes to file", async () => {
    const testPath = join(getTestEnvPath(), "nested", ".env");
    const result = await runCommand([
      "init",
      ...initFlags,
      "--output",
      testPath,
    ]);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain(`Configuration written to ${testPath}`);
    const content = await readFile(testPath, "utf-8");
    expect(content).toContain("XMTP_WALLET_KEY=0x");
    expect(content).toContain("XMTP_DB_ENCRYPTION_KEY=");
    expect(content).toContain(`XMTP_BACKEND_URL=${backendUrl}`);
    expect(content).toContain("XMTP_ENV=local");
    expect(content.match(/XMTP_WALLET_KEY=(0x[a-f0-9]{64})/i)).not.toBeNull();
    expect(
      content.match(/XMTP_DB_ENCRYPTION_KEY=([a-f0-9]{64})/i),
    ).not.toBeNull();
    expect((await stat(testPath)).mode & 0o077).toBe(0);
    expect((await stat(dirname(testPath))).mode & 0o077).toBe(0);
  });

  it("prints keys and a custom database label to stdout", async () => {
    const result = await runCommand([
      "init",
      ...initFlags,
      "--stdout",
      "--env",
      "staging-a",
    ]);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain("XMTP_WALLET_KEY=0x");
    expect(result.stdout).toContain("XMTP_DB_ENCRYPTION_KEY=");
    expect(result.stdout).toContain(`XMTP_BACKEND_URL=${backendUrl}`);
    expect(result.stdout).toContain("XMTP_ENV=staging-a");
  });

  it.each(["", ".", "..", "a/b", "a\\b"])(
    "rejects invalid database label %s before writing a file",
    async (label) => {
      const testPath = getTestEnvPath();
      const result = await runCommand([
        "init",
        ...initFlags,
        `--env=${label}`,
        "--output",
        testPath,
      ]);

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Environment label must be non-empty");
      await expect(readFile(testPath, "utf-8")).rejects.toThrow();
    },
  );

  it("refuses to overwrite without --force", async () => {
    const testPath = getTestEnvPath();
    const firstResult = await runCommand([
      "init",
      ...initFlags,
      "--output",
      testPath,
    ]);
    expect(firstResult.exitCode).toBe(0);

    const secondResult = await runCommand([
      "init",
      ...initFlags,
      "--output",
      testPath,
    ]);
    expect(secondResult.exitCode).not.toBe(0);
    expect(secondResult.stderr).toContain("File already exists");
    expect(secondResult.stderr).toContain("--force");
  });

  it("overwrites existing file with --force", async () => {
    const testPath = getTestEnvPath();
    const firstResult = await runCommand([
      "init",
      ...initFlags,
      "--output",
      testPath,
    ]);
    expect(firstResult.exitCode).toBe(0);
    expect(firstResult.stdout).toContain(
      `Configuration written to ${testPath}`,
    );
    const originalContent = await readFile(testPath, "utf-8");
    await chmod(testPath, 0o644);
    const oldHandle = await open(testPath);

    const result = await runCommand([
      "init",
      ...initFlags,
      "--output",
      testPath,
      "--force",
    ]);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain(`Configuration written to ${testPath}`);
    const updatedContent = await readFile(testPath, "utf-8");
    expect(updatedContent).not.toBe(originalContent);
    expect(updatedContent).toContain("XMTP_WALLET_KEY=0x");
    expect((await stat(testPath)).mode & 0o777).toBe(0o600);
    expect(await oldHandle.readFile("utf-8")).toBe(originalContent);
    await oldHandle.close();
  });
});
