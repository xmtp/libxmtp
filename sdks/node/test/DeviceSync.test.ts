import { randomUUID as uuid } from "node:crypto";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

describe("DeviceSync", () => {
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
