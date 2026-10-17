import { randomUUID as uuid } from "node:crypto";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

describe("DeviceSync", () => {
  // An archive key must be exactly 32 bytes: a longer key is rejected rather
  // than truncated, which would let every key sharing its first 32 bytes open
  // the archive.
  // verifies: ARCH-012
  it("should reject an archive key that is not 32 bytes", async () => {
    const { signer } = createSigner();
    const alix = await createRegisteredClient(signer);
    const path = join(tmpdir(), `xmtp-archive-${uuid()}.bin`);
    for (const length of [31, 33]) {
      await expect(
        alix.archives.exportToFile(path, new Uint8Array(length), undefined),
      ).rejects.toThrow("archive key must be exactly 32 bytes");
    }
  });

  it("should export and import a local archive", async () => {
    const { signer: boSigner } = createSigner();
    const { signer: alixSigner } = createSigner();
    const bo = await createRegisteredClient(boSigner, { deviceSync: true });
    const alix = await createRegisteredClient(alixSigner, { deviceSync: true });
    const group = await alix.conversations.createGroup([bo.inboxId]);
    const message = await group.sendText("archive me");
    const key = new Uint8Array(32).fill(1);
    const path = join(tmpdir(), `xmtp-archive-${uuid()}.bin`);

    try {
      await alix.archives.exportToFile(path, key, undefined);
      const metadata = await alix.archives.metadataFromFile(path, key);
      expect(metadata.elements.length).toBeGreaterThan(0);

      const alix2 = await createRegisteredClient(alixSigner, {
        deviceSync: true,
      });
      await alix2.archives.importFromFile(path, key);

      const restored = await alix2.conversations.getById(group.id);
      expect(restored).toBeTruthy();
      expect(
        (await restored!.messages()).some((entry) => entry.id === message),
      ).toBe(true);
    } finally {
      await rm(path, { force: true });
    }
  });
});
