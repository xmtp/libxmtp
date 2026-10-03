import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { Client } from "@xmtp/node-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

import { createSigner, createUser } from "@/user/User";

import { Agent } from "./Agent";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
});

describe("agent backend authentication", () => {
  it.each(["create", "createFromEnv"] as const)(
    "forwards backend.credentials through %s without invoking it",
    async (method) => {
      vi.stubEnv("XMTP_DB_DIRECTORY", undefined);
      vi.stubEnv("XMTP_BACKEND_URL", "https://backend.example.com");
      const key = `0x${"01".repeat(32)}` as const;
      vi.stubEnv("XMTP_WALLET_KEY", key);
      const stopped = new Error("client creation reached");
      const create = vi.spyOn(Client, "create").mockRejectedValue(stopped);
      const authCallback = vi.fn(async () => ({
        value: "Bearer secret",
        expiresAtSeconds: 1234567890n,
      }));
      const options = {
        backend: {
          url: "https://backend.example.com",
          credentials: { credential: authCallback },
        },
        storage: { location: "inMemory" as const },
      };
      await expect(
        method === "create"
          ? Agent.create(createSigner(createUser(key)), options)
          : Agent.createFromEnv(options),
      ).rejects.toBe(stopped);
      expect(create).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({
          ...options,
          backend: expect.objectContaining(options.backend),
        }),
      );
      expect(authCallback).not.toHaveBeenCalled();
    },
  );
});

describe("agent environment storage", () => {
  const key = `0x${"01".repeat(32)}` as const;
  const directories: string[] = [];

  function setup(directory: string) {
    vi.stubEnv("XMTP_DB_DIRECTORY", directory);
    vi.stubEnv("XMTP_ENV", "production");
    vi.stubEnv("XMTP_BACKEND_URL", "https://backend.example.com");
    vi.stubEnv("XMTP_WALLET_KEY", key);
    const stopped = new Error("client creation reached");
    const create = vi.spyOn(Client, "create").mockRejectedValue(stopped);
    return { create, stopped };
  }

  function directory() {
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), "agent-storage-"));
    directories.push(parent);
    return path.join(parent, "databases");
  }

  afterEach(() => {
    for (const parent of directories.splice(0)) {
      fs.rmSync(parent, { recursive: true, force: true });
    }
  });

  it("creates an absent directory and uses the data directory layout", async () => {
    const dbDirectory = directory();
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv()).rejects.toBe(stopped);
    expect(fs.statSync(dbDirectory).isDirectory()).toBe(true);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        storage: expect.objectContaining({
          location: { directory: dbDirectory },
          label: "production",
        }),
      }),
    );
  });

  it("reopens one legacy database through an explicit location", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    const dbPath = path.join(dbDirectory, `xmtp-${"a".repeat(64)}.db3`);
    fs.writeFileSync(dbPath, "legacy database");
    fs.writeFileSync(path.join(dbDirectory, "xmtp-not-an-inbox.db3"), "ignore");
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv()).rejects.toBe(stopped);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        storage: expect.objectContaining({
          location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
        }),
      }),
    );
  });

  it("rejects several legacy databases before opening a new location", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    for (const id of ["a", "b"]) {
      fs.writeFileSync(path.join(dbDirectory, `xmtp-${id.repeat(64)}.db3`), "legacy");
    }
    const { create } = setup(dbDirectory);
    await expect(Agent.createFromEnv()).rejects.toThrow(
      "More than one legacy XMTP database exists",
    );
    expect(create).not.toHaveBeenCalled();
  });

  it("uses caller storage even when legacy databases exist", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    for (const id of ["a", "b"]) {
      fs.writeFileSync(path.join(dbDirectory, `xmtp-${id.repeat(64)}.db3`), "legacy");
    }
    const storage = { location: "inMemory" as const };
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv({ storage })).rejects.toBe(stopped);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ storage }),
    );
  });
});
