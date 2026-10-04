import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { Client, generateInboxId } from "@xmtp/node-sdk";
import { toBytes } from "viem";
import { generatePrivateKey } from "viem/accounts";
import { afterEach, describe, expect, it, vi } from "vitest";

import { createIdentifier, createSigner, createUser } from "@/user/User";

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
  const inboxId = generateInboxId(createIdentifier(createUser(key)));
  const otherInboxId = generateInboxId(
    createIdentifier(createUser(`0x${"02".repeat(32)}`)),
  );

  function setup(directory?: string) {
    vi.stubEnv("XMTP_DB_DIRECTORY", directory);
    vi.stubEnv("XMTP_ENV", "production");
    vi.stubEnv("XMTP_BACKEND_URL", "https://backend.example.com");
    vi.stubEnv("XMTP_WALLET_KEY", key);
    const stopped = new Error("client creation reached");
    const create = vi.spyOn(Client, "create").mockRejectedValue(stopped);
    vi.spyOn(Client, "inboxIdFor").mockResolvedValue(inboxId);
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
    const dbPath = path.join(dbDirectory, `xmtp-${inboxId}.db3`);
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

  it("reopens a symlinked legacy database through its existing path", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    const targetPath = path.join(dbDirectory, "legacy-target.db3");
    fs.writeFileSync(targetPath, "legacy database");
    const dbPath = path.join(dbDirectory, `xmtp-${inboxId}.db3`);
    fs.symlinkSync(targetPath, dbPath);
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

  it("rejects a dangling legacy database symlink before client creation", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    fs.symlinkSync(
      path.join(dbDirectory, "missing.db3"),
      path.join(dbDirectory, `xmtp-${inboxId}.db3`),
    );
    const { create } = setup(dbDirectory);

    await expect(Agent.createFromEnv()).rejects.toMatchObject({
      code: "ENOENT",
    });
    expect(create).not.toHaveBeenCalled();
  });

  it("reopens one legacy default database from the working directory", async () => {
    const workingDirectory = directory();
    fs.mkdirSync(workingDirectory);
    const dbPath = path.join(
      workingDirectory,
      `xmtp-production-${inboxId}.db3`,
    );
    fs.writeFileSync(dbPath, "legacy database");
    fs.writeFileSync(
      path.join(workingDirectory, `xmtp-development-${otherInboxId}.db3`),
      "other environment",
    );
    vi.spyOn(process, "cwd").mockReturnValue(workingDirectory);
    const { create, stopped } = setup();
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

  it("selects an unregistered legacy database with the old default nonce", async () => {
    const workingDirectory = directory();
    fs.mkdirSync(workingDirectory);
    const legacyInboxId = generateInboxId(
      createIdentifier(createUser(key)),
      1n,
    );
    const dbPath = path.join(
      workingDirectory,
      `xmtp-production-${legacyInboxId}.db3`,
    );
    fs.writeFileSync(dbPath, "legacy database");
    vi.spyOn(process, "cwd").mockReturnValue(workingDirectory);
    const { create, stopped } = setup();

    await expect(Agent.createFromEnv()).rejects.toBe(stopped);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        registration: expect.objectContaining({ nonce: 1n }),
        storage: expect.objectContaining({
          location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
        }),
      }),
    );
  });

  it.each(["directory", "default"] as const)(
    "rejects a %s legacy database when current storage has the same inbox",
    async (location) => {
      const dbDirectory = directory();
      fs.mkdirSync(dbDirectory);
      if (location === "default")
        vi.spyOn(process, "cwd").mockReturnValue(dbDirectory);
      else vi.stubEnv("XMTP_DB_DIRECTORY", dbDirectory);
      const legacyRoot = dbDirectory;
      const dbPath = path.join(
        legacyRoot,
        location === "default"
          ? `xmtp-production-${inboxId}.db3`
          : `xmtp-${inboxId}.db3`,
      );
      fs.writeFileSync(dbPath, "legacy database");
      const currentRoot =
        location === "default" ? path.join(dbDirectory, "xmtp") : dbDirectory;
      const currentPath = path.join(
        currentRoot,
        "production",
        "deployment",
        inboxId,
        "xmtp.db3",
      );
      fs.mkdirSync(path.dirname(currentPath), { recursive: true });
      fs.writeFileSync(currentPath, "current database");
      const { create } = setup(
        location === "directory" ? dbDirectory : undefined,
      );

      await expect(Agent.createFromEnv()).rejects.toThrow(
        "Both legacy and current XMTP databases",
      );
      expect(create).not.toHaveBeenCalled();
    },
  );

  it("ignores unrelated legacy databases in a named directory", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    for (const id of [inboxId, otherInboxId]) {
      fs.writeFileSync(path.join(dbDirectory, `xmtp-${id}.db3`), "legacy");
    }
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv()).rejects.toBe(stopped);
    const dbPath = path.join(dbDirectory, `xmtp-${inboxId}.db3`);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        storage: expect.objectContaining({
          location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
        }),
      }),
    );
  });

  it("ignores unrelated legacy databases in the working directory", async () => {
    const workingDirectory = directory();
    fs.mkdirSync(workingDirectory);
    for (const id of [inboxId, otherInboxId]) {
      fs.writeFileSync(
        path.join(workingDirectory, `xmtp-production-${id}.db3`),
        "legacy",
      );
    }
    vi.spyOn(process, "cwd").mockReturnValue(workingDirectory);
    const { create, stopped } = setup();
    await expect(Agent.createFromEnv()).rejects.toBe(stopped);
    const dbPath = path.join(
      workingDirectory,
      `xmtp-production-${inboxId}.db3`,
    );
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        storage: expect.objectContaining({
          location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
        }),
      }),
    );
  });

  it("rejects two legacy environments for the selected inbox", async () => {
    const workingDirectory = directory();
    fs.mkdirSync(workingDirectory);
    for (const environment of ["production", "development"]) {
      fs.writeFileSync(
        path.join(workingDirectory, `xmtp-${environment}-${inboxId}.db3`),
        "legacy",
      );
    }
    vi.spyOn(process, "cwd").mockReturnValue(workingDirectory);
    const { create } = setup();
    vi.stubEnv("XMTP_ENV", undefined);
    await expect(Agent.createFromEnv()).rejects.toThrow(
      "More than one legacy XMTP database exists",
    );
    expect(create).not.toHaveBeenCalled();
  });

  it("uses caller storage even when legacy databases exist", async () => {
    const dbDirectory = directory();
    fs.mkdirSync(dbDirectory);
    for (const id of ["a", "b"]) {
      fs.writeFileSync(
        path.join(dbDirectory, `xmtp-${id.repeat(64)}.db3`),
        "legacy",
      );
    }
    const storage = { location: "inMemory" as const };
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv({ storage })).rejects.toBe(stopped);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ storage }),
    );
  });

  it("does not create an environment directory for caller storage", async () => {
    const dbDirectory = directory();
    const storage = { location: "inMemory" as const };
    const { create, stopped } = setup(dbDirectory);
    await expect(Agent.createFromEnv({ storage })).rejects.toBe(stopped);
    expect(create).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ storage }),
    );
    expect(fs.existsSync(dbDirectory)).toBe(false);
  });

  it.each(["03".repeat(32), "not-a-key"])(
    "uses a caller encryption key instead of environment key %s",
    async (environmentKey) => {
      const { create, stopped } = setup();
      vi.stubEnv("XMTP_DB_ENCRYPTION_KEY", environmentKey);
      const encryptionKey = toBytes(`0x${"02".repeat(32)}`);
      const storage = { location: "inMemory" as const, encryptionKey };
      await expect(Agent.createFromEnv({ storage })).rejects.toBe(stopped);
      expect(create).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({ storage: expect.objectContaining(storage) }),
      );
    },
  );

  it.each(["", "0x"])(
    "rejects an empty environment encryption key %j",
    async (environmentKey) => {
      const { create } = setup();
      vi.stubEnv("XMTP_DB_ENCRYPTION_KEY", environmentKey);
      await expect(Agent.createFromEnv()).rejects.toThrow(
        "XMTP_DB_ENCRYPTION_KEY must contain 32 bytes.",
      );
      expect(create).not.toHaveBeenCalled();
    },
  );

  it.each(["directory", "default"] as const)(
    "opens an encrypted %s legacy database with the same installation",
    async (location) => {
      const backendUrl = process.env.XMTP_BACKEND_URL;
      if (!backendUrl) throw new Error("XMTP_BACKEND_URL is required");
      const dbDirectory = directory();
      fs.mkdirSync(dbDirectory);
      const walletKey = generatePrivateKey();
      const user = createUser(walletKey);
      const preview = await Client.create(createSigner(user), {
        backend: { url: backendUrl },
        deviceSync: false,
        storage: { location: "inMemory" },
      });
      const inboxId = preview.inboxId;
      await preview.end();
      const dbPath = path.join(
        dbDirectory,
        location === "directory"
          ? `xmtp-${inboxId}.db3`
          : `xmtp-production-${inboxId}.db3`,
      );
      const encryptionKeyHex = "02".repeat(32);
      const encryptionKey = toBytes(`0x${encryptionKeyHex}`);
      const first = await Client.create(createSigner(user), {
        backend: { url: backendUrl },
        deviceSync: false,
        storage: {
          location: { dbPath, attachmentsDir: `${dbPath}.attachments` },
          encryptionKey,
        },
      });
      expect(first.inboxId).toBe(inboxId);
      const installationId = first.installationId;
      await first.end();
      expect(fs.statSync(dbPath).isFile()).toBe(true);

      vi.stubEnv("XMTP_WALLET_KEY", walletKey);
      vi.stubEnv(
        "XMTP_DB_DIRECTORY",
        location === "directory" ? dbDirectory : undefined,
      );
      if (location === "default")
        vi.spyOn(process, "cwd").mockReturnValue(dbDirectory);
      vi.stubEnv("XMTP_DB_ENCRYPTION_KEY", encryptionKeyHex);
      vi.stubEnv("XMTP_ENV", "production");
      vi.stubEnv("XMTP_BACKEND_URL", backendUrl);
      const agent = await Agent.createFromEnv({ deviceSync: false });
      try {
        expect(agent.client.inboxId).toBe(inboxId);
        expect(agent.client.installationId).toBe(installationId);
        expect(
          fs.existsSync(
            path.join(
              dbDirectory,
              location === "directory" ? "production" : "xmtp",
            ),
          ),
        ).toBe(false);
      } finally {
        await agent.client.end();
      }
    },
  );
});
