import { randomUUID } from "node:crypto";
import {
  copyFileSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import {
  buildClient,
  clientOptions,
  createClient,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
import {
  Client,
  XmtpError,
  generateInboxId,
  latestInboxUpdatesCount,
} from "@xmtp/node-sdk";
import { uint8ArrayToHex } from "uint8array-extras";
import { describe, expect, it } from "vitest";

describe("Client", () => {
  it("does not expose a signer error while checking legacy storage", async () => {
    const { signer } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-signer-error-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      writeFileSync(
        join(directory, `xmtp-production-${"a".repeat(64)}.db3`),
        "legacy candidate",
      );
      const privateText = "wallet-private-error-text";
      const failingSigner = {
        ...signer,
        identity: async () => {
          throw new Error(privateText);
        },
      };
      let failure: unknown;
      try {
        await Client.create(
          failingSigner,
          clientOptions({
            allowOffline: true,
            storage: { location: "default" },
          }),
        );
      } catch (error) {
        failure = error;
      }
      expect(failure).toBeInstanceOf(XmtpError.InvalidArgument);
      expect(JSON.stringify(failure)).not.toContain(privateText);
      expect(String(failure)).not.toContain(privateText);
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("reopens an unregistered legacy database with the old default nonce", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-legacy-nonce-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const inboxId = generateInboxId(identifier, 1n);
      const oldPath = join(directory, `xmtp-production-${inboxId}.db3`);
      const first = await Client.create(
        signer,
        clientOptions({
          registration: { auto: false, nonce: 1n },
          storage: {
            location: {
              dbPath: oldPath,
              attachmentsDir: `${oldPath}.attachments`,
            },
          },
        }),
      );
      await first.end();

      const reopened = await Client.create(
        signer,
        clientOptions({
          registration: { auto: false },
          storage: { location: "default", label: "production" },
        }),
      );
      try {
        expect(reopened.storagePath).toBe(realpathSync(oldPath));
        expect(reopened.inboxId).toBe(inboxId);
      } finally {
        await reopened.end();
      }
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("reuses the legacy default database and rejects ambiguous matches", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-legacy-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const inboxId = generateInboxId(identifier);
      const oldPath = join(directory, `xmtp-production-${inboxId}.db3`);
      const otherPath = join(directory, `xmtp-local-${inboxId}.db3`);
      const options = clientOptions({
        registration: { auto: false },
        storage: {
          location: {
            dbPath: oldPath,
            attachmentsDir: `${oldPath}.attachments`,
          },
        },
      });
      const first = await Client.create(signer, options);
      await first.end();

      const withDefault = {
        ...options,
        storage: { location: "default" as const },
      };
      const reopened = await Client.create(signer, withDefault);
      try {
        expect(reopened.storagePath).toBe(realpathSync(oldPath));
      } finally {
        await reopened.end();
      }

      copyFileSync(oldPath, otherPath);
      await expect(Client.create(signer, withDefault)).rejects.toThrow(
        "More than one legacy XMTP database",
      );
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("builds from a registered legacy default database", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-legacy-build-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const oldPath = join(
        directory,
        `xmtp-local-${generateInboxId(identifier)}.db3`,
      );
      const options = clientOptions({
        storage: {
          location: {
            dbPath: oldPath,
            attachmentsDir: `${oldPath}.attachments`,
          },
        },
      });
      const registered = await Client.create(signer, options);
      const inboxId = registered.inboxId;
      await registered.end();

      const built = await Client.build(
        identifier,
        { ...options, storage: { location: "default" } },
        inboxId,
      );
      try {
        expect(built.storagePath).toBe(realpathSync(oldPath));
      } finally {
        await built.end();
      }
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("reuses only the matching labeled legacy database", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-labeled-legacy-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const inboxId = generateInboxId(identifier);
      const oldPath = join(directory, `xmtp-production-${inboxId}.db3`);
      const otherPath = join(directory, `xmtp-local-${inboxId}.db3`);
      const options = clientOptions({
        registration: { auto: false },
        storage: {
          location: {
            dbPath: oldPath,
            attachmentsDir: `${oldPath}.attachments`,
          },
        },
      });
      const first = await Client.create(signer, options);
      await first.end();
      copyFileSync(oldPath, otherPath);

      const reopened = await Client.create(signer, {
        ...options,
        storage: { location: "default", label: "production" },
      });
      try {
        expect(reopened.storagePath).toBe(realpathSync(oldPath));
      } finally {
        await reopened.end();
      }
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("rejects a labeled legacy database that conflicts with a current database", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(
      join(tmpdir(), "xmtp-node-labeled-conflict-"),
    );
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const options = clientOptions({
        registration: { auto: false },
        storage: { location: "default", label: "production" },
      });
      const current = await Client.create(signer, options);
      const currentPath = current.storagePath;
      await current.end();
      if (currentPath === undefined)
        throw new Error("default database path is missing");

      copyFileSync(
        currentPath,
        join(directory, `xmtp-production-${generateInboxId(identifier)}.db3`),
      );
      await expect(Client.create(signer, options)).rejects.toThrow(
        "Both legacy and current XMTP databases",
      );
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("rejects a legacy database when a current default database also exists", async () => {
    const { signer, identifier } = createSigner();
    const directory = mkdtempSync(join(tmpdir(), "xmtp-node-legacy-conflict-"));
    const previousDirectory = process.cwd();
    process.chdir(directory);
    try {
      const options = clientOptions({
        registration: { auto: false },
        storage: { location: "default" },
      });
      const current = await Client.create(signer, options);
      const currentPath = current.storagePath;
      await current.end();
      if (currentPath === undefined)
        throw new Error("default database path is missing");

      copyFileSync(
        currentPath,
        join(directory, `xmtp-production-${generateInboxId(identifier)}.db3`),
      );
      await expect(Client.create(signer, options)).rejects.toThrow(
        "Both legacy and current XMTP databases",
      );
    } finally {
      process.chdir(previousDirectory);
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("matches an omitted nonce in inbox calculation and client creation", async () => {
    const { signer, identifier } = createSigner();
    const client = await createClient(signer);
    try {
      expect(generateInboxId(identifier)).toBe(client.inboxId);
      expect(generateInboxId(identifier, 0n)).toBe(client.inboxId);
      expect(generateInboxId(identifier, 1n)).not.toBe(client.inboxId);
    } finally {
      await client.end();
    }
  });

  it("creates an unregistered client and preserves the selected identity and nonce", async () => {
    const { signer, identifier } = createSigner();
    const client = await createClient(signer, { registration: { nonce: 1n } });
    expect(client.identity).toEqual(identifier);
    expect(await client.isRegistered()).toBe(false);
    expect(client.installationId).toBeDefined();
    const same = await createClient(signer, { registration: { nonce: 1n } });
    const other = await createClient(signer, { registration: { nonce: 2n } });
    expect(same.inboxId).toBe(client.inboxId);
    expect(other.inboxId).not.toBe(client.inboxId);
  });
  it("builds a client without a signer and permits static identity reads", async () => {
    const { signer, identifier } = createSigner();
    const options = clientOptions();
    const registered = await createRegisteredClient(signer, options);
    await registered.end();
    const built = await buildClient(identifier, options);
    expect(built.inboxId).toBe(registered.inboxId);
    expect(built.identity).toEqual(identifier);
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
  it("preserves a shared backend and worker intervals", async () => {
    const backend = {
      url: process.env.XMTP_BACKEND_URL!,
      appVersion: "test/8",
    };
    const workers = {
      defaultIntervalNs: 60_000_000_000n,
      intervals: [{ kind: "deviceSync" as const, intervalNs: 30_000_000_000n }],
    };
    const first = await createClient(createSigner().signer, {
      backend,
      workers,
    });
    const second = await createClient(createSigner().signer, { backend });
    expect(first.options.backend).toEqual(backend);
    expect(first.options.workers).toEqual(workers);
    expect(second.appVersion).toBe("test/8");
    expect(second.inboxId).not.toBe(first.inboxId);
  });
  it("should return a version", async () => {
    const { signer } = createSigner();
    const client = await createClient(signer);
    expect(client.libxmtpVersion.length).toBeGreaterThan(0);
    expect(client.libxmtpVersion).toBeDefined();
  });

  it("should register an identity", async () => {
    const { signer } = createSigner();
    await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer);
    expect(await client2.isRegistered()).toBe(true);
  });

  it("should be able to message a registered identity", async () => {
    const { signer, address } = createSigner();
    const client = await createRegisteredClient(signer);
    const canMessage = await client.canMessage([await signer.identity()]);
    expect(Object.fromEntries(canMessage)).toEqual({
      [`ethereum:${address}`]: true,
    });
  });

  it("should be able to check if an identifier can be messaged without a client instance", async () => {
    const { signer, address } = createSigner();
    await createRegisteredClient(signer);
    const canMessage = await Client.canMessage([await signer.identity()], {
      url: process.env.XMTP_BACKEND_URL!,
    });
    expect(Object.fromEntries(canMessage)).toEqual({
      [`ethereum:${address}`]: true,
    });
  });

  it("should get an inbox ID from an address", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const inboxId = await client.inboxIdFor(await signer.identity());
    expect(inboxId).toBe(client.inboxId);
  });

  it("should add a wallet association to the client", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);

    await client.unsafeAddAccount(signer2, true);

    const inboxState = await client.inboxState(false);
    expect(inboxState.identities.length).toEqual(2);
    expect(inboxState.identities).toContainEqual(await signer.identity());
    expect(inboxState.identities).toContainEqual(await signer2.identity());
  });

  it("should remove a wallet association from the client", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);

    await client.unsafeAddAccount(signer2, true);
    await client.removeAccount(signer, await signer2.identity());

    const inboxState = await client.inboxState(false);
    expect(inboxState.identities).toEqual([await signer.identity()]);
  });

  it("should revoke specific installations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer, {});
    const client3 = await createRegisteredClient(signer, {});

    const inboxState = await client3.inboxState(true);
    expect(inboxState.installations.length).toBe(3);

    const installationIds = inboxState.installations.map((i) => i.id);
    expect(installationIds).toContain(client.installationId);
    expect(installationIds).toContain(client2.installationId);
    expect(installationIds).toContain(client3.installationId);

    await client3.revokeInstallations(signer, [client.installationId]);

    const inboxState2 = await client3.inboxState(true);

    expect(inboxState2.installations.length).toBe(2);

    const installationIds2 = inboxState2.installations.map((i) => i.id);
    expect(installationIds2).toContain(client2.installationId);
    expect(installationIds2).toContain(client3.installationId);
    expect(installationIds2).not.toContain(client.installationId);
  });

  it("should revoke all other installations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer, {});
    const client3 = await createRegisteredClient(signer, {});

    const inboxState = await client3.inboxState(true);
    expect(inboxState.installations.length).toBe(3);

    const installationIds = inboxState.installations.map((i) => i.id);
    expect(installationIds).toContain(client.installationId);
    expect(installationIds).toContain(client2.installationId);
    expect(installationIds).toContain(client3.installationId);

    await client3.revokeAllOtherInstallations(signer);

    const inboxState2 = await client3.inboxState(true);

    expect(inboxState2.installations.length).toBe(1);
    expect(inboxState2.installations[0].id).toBe(client3.installationId);
  });

  it("should not fail when revoking all other installations with only one installation", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);

    const inboxState = await client.inboxState(true);
    expect(inboxState.installations.length).toBe(1);
    expect(inboxState.installations[0].id).toBe(client.installationId);

    await expect(
      client.revokeAllOtherInstallations(signer),
    ).resolves.not.toThrow();
  });

  it("should statically revoke specific installations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer, {});
    const client3 = await createRegisteredClient(signer, {});

    const inboxState = await client3.inboxState(true);
    expect(inboxState.installations.length).toBe(3);

    const installationIds = inboxState.installations.map((i) => i.id);
    expect(installationIds).toContain(client.installationId);
    expect(installationIds).toContain(client2.installationId);
    expect(installationIds).toContain(client3.installationId);

    await Client.revokeInstallations(
      signer,
      client3.inboxId,
      [client.installationId],
      { url: process.env.XMTP_BACKEND_URL! },
    );

    const inboxState2 = await client3.inboxState(true);

    expect(inboxState2.installations.length).toBe(2);

    const installationIds2 = inboxState2.installations.map((i) => i.id);
    expect(installationIds2).toContain(client2.installationId);
    expect(installationIds2).toContain(client3.installationId);
    expect(installationIds2).not.toContain(client.installationId);
  });

  it("should throw when trying to create more than 10 installations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer, {});
    const client3 = await createRegisteredClient(signer, {});
    const client4 = await createRegisteredClient(signer, {});
    const client5 = await createRegisteredClient(signer, {});
    const client6 = await createRegisteredClient(signer, {});
    const client7 = await createRegisteredClient(signer, {});
    const client8 = await createRegisteredClient(signer, {});
    const client9 = await createRegisteredClient(signer, {});
    const client10 = await createRegisteredClient(signer, {});

    const inboxState = await client3.inboxState(true);
    expect(inboxState.installations.length).toBe(10);

    const installationIds = inboxState.installations.map((i) => i.id);
    expect(installationIds).toContain(client.installationId);
    expect(installationIds).toContain(client2.installationId);
    expect(installationIds).toContain(client3.installationId);
    expect(installationIds).toContain(client4.installationId);
    expect(installationIds).toContain(client5.installationId);
    expect(installationIds).toContain(client6.installationId);
    expect(installationIds).toContain(client7.installationId);
    expect(installationIds).toContain(client8.installationId);
    expect(installationIds).toContain(client9.installationId);
    expect(installationIds).toContain(client10.installationId);

    await expect(createRegisteredClient(signer, {})).rejects.toThrow();
  });

  it("should verify signatures", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const signatureText = "gm1";
    const signature = await client.signWithInstallationKey(signatureText);
    const verified = await client.verifySignedWithInstallationKey(
      signatureText,
      signature,
    );
    expect(verified).toBe(true);
    const verified2 = await Client.verifySignedWithPublicKey(
      signatureText,
      signature,
      client.installationIdBytes,
    );
    expect(verified2).toBe(true);

    const signatureText2 = new Uint8Array(32).fill(1);
    const signature2 = await client.signWithInstallationKey(
      uint8ArrayToHex(signatureText2),
    );
    const verified3 = await Client.verifySignedWithPublicKey(
      uint8ArrayToHex(signatureText2),
      signature2,
      client.installationIdBytes,
    );
    expect(verified3).toBe(true);
    const verified4 = await Client.verifySignedWithPublicKey(
      uint8ArrayToHex(signatureText2),
      signature,
      client.installationIdBytes,
    );
    expect(verified4).toBe(false);
  });

  it("should check if an address is authorized", async () => {
    const { signer, address } = createSigner();
    const client = await createRegisteredClient(signer);
    const authorized = await Client.isAddressAuthorized(
      client.inboxId,
      address,
      { url: process.env.XMTP_BACKEND_URL! },
    );
    expect(authorized).toBe(true);

    const authorized2 = await Client.isAddressAuthorized(
      client.inboxId,
      "0x1234567890123456789012345678901234567890",
      { url: process.env.XMTP_BACKEND_URL! },
    );
    expect(authorized2).toBe(false);
  });

  it("should check if an installation is authorized", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const authorized = await Client.isInstallationAuthorized(
      client.inboxId,
      client.installationId,
      { url: process.env.XMTP_BACKEND_URL! },
    );
    expect(authorized).toBe(true);

    const authorized2 = await Client.isInstallationAuthorized(
      client.inboxId,
      "00".repeat(32),
      { url: process.env.XMTP_BACKEND_URL! },
    );
    expect(authorized2).toBe(false);
  });

  it("should change the recovery identifier", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);

    const inboxState = await client.inboxState(false);
    expect(inboxState.recoveryIdentity).toEqual(await signer.identity());

    await client.changeRecoveryIdentifier(signer, await signer2.identity());

    const inboxState2 = await client.inboxState(false);
    expect(inboxState2.recoveryIdentity).toEqual(await signer2.identity());
  });

  it("should read key package lifetime for specific installations", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer, {});
    const client3 = await createRegisteredClient(signer, {});

    const inboxState = await client3.inboxState(true);
    expect(inboxState.installations.length).toBe(3);

    const keyPackageStatuses = await client3.keyPackageStatuses([
      client.installationId,
      client2.installationId,
      client3.installationId,
    ]);
    expect(
      (keyPackageStatuses.get(client.installationId)!.lifetime?.notAfter ??
        0n) -
        (keyPackageStatuses.get(client.installationId)!.lifetime?.notBefore ??
          0n),
    ).toEqual(BigInt(3600 * 24 * 28 * 3 + 3600));
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

  it("should get inbox states from inbox IDs without a client", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer2);
    const inboxStates = await Client.inboxStates([client.inboxId], {
      url: process.env.XMTP_BACKEND_URL!,
    });
    expect(inboxStates.length).toBe(1);
    expect(inboxStates[0].inboxId).toBe(client.inboxId);
    expect(inboxStates[0].identities).toEqual([await signer.identity()]);

    const inboxStates2 = await Client.inboxStates([client2.inboxId], {
      url: process.env.XMTP_BACKEND_URL!,
    });
    expect(inboxStates2.length).toBe(1);
    expect(inboxStates2[0].inboxId).toBe(client2.inboxId);
    expect(inboxStates2[0].identities).toEqual([await signer2.identity()]);
  });

  it("should get latest inbox updates count from inbox IDs without a client", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const inboxUpdatesCounts = await latestInboxUpdatesCount([client.inboxId], {
      url: process.env.XMTP_BACKEND_URL!,
    });
    expect(inboxUpdatesCounts.get(client.inboxId)).toBeTypeOf("bigint");
  });

  it("should get own inbox updates count from a client", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const inboxUpdatesCounts = await client.latestInboxUpdatesCount(
      [client.inboxId],
      true,
    );
    const ownInboxUpdatesCount = await client.ownInboxUpdatesCount(true);
    expect(inboxUpdatesCounts.get(client.inboxId)).toBe(ownInboxUpdatesCount);

    expect(inboxUpdatesCounts.get(client.inboxId)).toBeTypeOf("bigint");
    expect(ownInboxUpdatesCount).toBeTypeOf("bigint");
  });

  it("should transfer an identifier to a new inbox", async () => {
    // original signer
    const { signer, identifier } = createSigner();
    // temporary signer
    const { signer: signer2, identifier: identifier2 } = createSigner();
    const client = await createRegisteredClient(signer);
    // add temporary account
    await client.unsafeAddAccount(signer2, true);
    // remove existing account
    await client.removeAccount(signer, identifier);
    // change recovery identifier to temporary account
    await client.changeRecoveryIdentifier(signer, identifier2);

    const inboxState = await client.inboxState(true);
    // check that the temporary account is the only account on the inbox
    expect(inboxState.identities).toEqual([identifier2]);
    expect(inboxState.recoveryIdentity).toEqual(identifier2);

    // temporary signer
    const { signer: signer3, identifier: identifier3 } = createSigner();
    // create client to transfer original account to
    const transferClient = await createRegisteredClient(signer3);
    // add original account to transfer client
    await transferClient.unsafeAddAccount(signer, true);
    // remove temporary transfer identifier
    await transferClient.removeAccount(signer3, identifier3);
    // change recovery identifier to original account
    await transferClient.changeRecoveryIdentifier(signer3, identifier);

    const inboxState2 = await transferClient.inboxState(true);
    // check that the original account is the only account on the inbox
    expect(inboxState2.identities).toEqual([identifier]);
    expect(inboxState2.recoveryIdentity).toEqual(identifier);

    // check that the inbox IDs are different
    expect(client.inboxId).not.toBe(transferClient.inboxId);

    // ensure that a client can be created with the original signer
    const client2 = await createRegisteredClient(signer, {
      // must use a different db path to avoid errors
    });
    expect(client2.inboxId).toBe(transferClient.inboxId);
  });
});
