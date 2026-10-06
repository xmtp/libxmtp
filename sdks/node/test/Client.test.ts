import { randomUUID } from "node:crypto";
import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

import {
  buildClient,
  clientOptions,
  createClient,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
import { Client, XmtpError, generateInboxId } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

describe("Client", () => {
  it("keeps default storage in the current data directory when an old database exists", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-default-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const oldPath = join(
        directory,
        `xmtp-production-${generateInboxId(identifier)}.db3`,
      );
      const oldClient = await Client.create(
        signer,
        clientOptions({
          registration: { auto: false },
          storage: {
            location: {
              dbPath: oldPath,
              attachmentsDir: `${oldPath}.attachments`,
            },
          },
        }),
      );
      await oldClient.end();

      const current = await Client.create(
        signer,
        clientOptions({
          registration: { auto: false },
          storage: { location: "default" },
        }),
      );
      try {
        expect(current.storagePath).not.toBe(realpathSync(oldPath));
        expect(
          current.storagePath?.startsWith(
            `${join(realpathSync(directory), "xmtp")}${sep}`,
          ),
        ).toBe(true);
      } finally {
        await current.end();
      }
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("builds a client without a signer and runs the static identity calls", async () => {
    const { signer, identifier, address } = createSigner();
    const backend = { url: process.env.XMTP_BACKEND_URL! };
    const options = clientOptions({ backend });
    const registered = await createRegisteredClient(signer, options);
    const { inboxId, installationId } = registered;
    const signature = await registered.signWithInstallationKey("gm");
    const installationKey = registered.installationIdBytes;
    await registered.end();
    expect(
      await Client.verifySignedWithPublicKey("gm", signature, installationKey),
    ).toBe(true);
    const built = await buildClient(identifier, options);
    expect(built.inboxId).toBe(inboxId);
    expect(built.identity).toEqual(identifier);
    await built.end();
    // The static wrappers in runtime/ts/client.ts move the backend argument to
    // the front. Only a live call catches an argument swap.
    expect(await Client.inboxIdFor(identifier, backend)).toBe(inboxId);
    expect(await Client.isAddressAuthorized(inboxId, address, backend)).toBe(
      true,
    );
    expect(
      await Client.isAddressAuthorized(
        inboxId,
        "0x1234567890123456789012345678901234567890",
        backend,
      ),
    ).toBe(false);
    expect(
      await Client.isInstallationAuthorized(inboxId, installationId, backend),
    ).toBe(true);
    expect(
      await Client.isInstallationAuthorized(inboxId, "00".repeat(32), backend),
    ).toBe(false);

    const other = await createRegisteredClient(signer, { backend });
    try {
      await Client.revokeInstallations(
        signer,
        inboxId,
        [installationId],
        backend,
      );
      const [state] = await Client.inboxStates([inboxId], backend);
      expect(state.inboxId).toBe(inboxId);
      expect(state.installations.map((i) => i.id)).toEqual([
        other.installationId,
      ]);
      // The instance lookup returns an optional inbox ID. An identity with no
      // inbox lifts to undefined, not null. (The static lookup above computes
      // the nonce-0 inbox ID instead.)
      expect(await other.inboxIdFor(createSigner().identifier)).toBeUndefined();
    } finally {
      await other.end();
    }
  });

  it("uses the selected storage pool and encryption key", async () => {
    const storage = {
      location: {
        dbPath: `./test-${randomUUID()}.db3`,
        attachmentsDir: `./test-${randomUUID()}.attachments`,
      },
      encryptionKey: new Uint8Array(32),
      pool: { min: 1, max: 2 },
    };
    const client = await createClient(createSigner().signer, { storage });
    expect(client.options.storage.location).toEqual(storage.location);
    expect(client.options.storage.pool).toEqual(storage.pool);
    expect(client.options.storage.encryptionKey).toBeUndefined();
    expect(client.storagePath).toBe(resolve(storage.location.dbPath));
  });

  it("reconnects the same live client storage and keeps its history", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const storage = client.storage;
    const inboxId = client.inboxId;
    try {
      const path = await storage.path();
      expect(path).toBe(client.storagePath);
      const group = await client.conversations.createGroup([]);
      const messageId = await group.sendText("stored before reconnect");
      await storage.reconnect();
      expect(await storage.path()).toBe(path);
      expect(client.inboxId).toBe(inboxId);
      expect((await client.conversations.getMessageById(messageId))?.id).toBe(
        messageId,
      );
    } finally {
      await client.end();
    }
    await expect(storage.reconnect()).rejects.toBeInstanceOf(
      XmtpError.ClientClosed,
    );
  });

  it("confirms registration before it reports success and permits a repeat", async () => {
    const client = await createClient(createSigner().signer);
    try {
      expect(await client.isRegistered()).toBe(false);
      await client.register();
      expect(await client.isRegistered()).toBe(true);
      await client.register();
      expect(await client.isRegistered()).toBe(true);
    } finally {
      await client.end();
    }
  });

  it("rejects operations after the client ends", async () => {
    const client = await createClient(createSigner().signer);
    await client.end();
    await expect(client.conversations.list()).rejects.toBeInstanceOf(
      XmtpError.ClientClosed,
    );
  });

  it("should close the client idempotently", async () => {
    const { signer } = createSigner();
    const client = await createClient(signer);
    await client.end();
    // a second call must resolve without throwing
    await expect(client.end()).resolves.not.toThrow();
  });
});
