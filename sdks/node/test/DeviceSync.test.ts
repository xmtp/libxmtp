import { randomUUID as uuid } from "node:crypto";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it, vi } from "vitest";

// Device sync hands work to background workers on both installations, so
// cross-installation visibility converges rather than completing on any
// single sync call. Poll (re-triggering the syncs) until the expected state
// appears instead of pacing with fixed sleeps — a fixed sleep loses the race
// on loaded CI runners.
const WAIT = { timeout: 30_000, interval: 1000 };

describe("DeviceSync", () => {
  it("should sync consent across installations", async () => {
    const { signer: boSigner } = createSigner();
    const { signer: alixSigner } = createSigner();

    const bo = await createRegisteredClient(boSigner, { deviceSync: true });

    const alix = await createRegisteredClient(alixSigner, { deviceSync: true });

    // create DM conversation
    const dm = await alix.conversations.createDm(bo.inboxId);
    const initialConsent = (await dm.state()).consentState;
    expect(initialConsent === "unknown" || initialConsent === "allowed").toBe(
      true,
    );

    await bo.conversations.sync();

    // create second installation for alix
    const alix2 = await createRegisteredClient(alixSigner, {
      deviceSync: true,
    });

    // the new installation's registration propagates asynchronously
    await vi.waitFor(async () => {
      const state = await alix2.inboxState(true);
      expect(state.installations.length).toBe(2);
    }, WAIT);

    // sync the DM on alix so conversation is pushed
    await dm.sync();
    await alix.conversations.syncAll(undefined);

    // alix2 syncs until it has the DM; re-trigger the sender side too
    const dm2 = await vi.waitFor(async () => {
      await alix.conversations.syncAll(undefined);
      await alix2.conversations.sync();
      const c = await alix2.conversations.getDmByInboxId(bo.inboxId);
      expect(c).toBeTruthy();
      return c!;
    }, WAIT);

    const consentOnAlix2Before = (await dm2.state()).consentState;
    expect(
      consentOnAlix2Before === "unknown" || consentOnAlix2Before === "allowed",
    ).toBe(true);

    // update consent to denied on alix
    await dm.updateConsentState("denied");
    const consentState = (await dm.state()).consentState;
    expect(consentState).toBe("denied");

    // The consent update is published into the device sync group once — if
    // that happens before alix2 has joined, alix2 can never decrypt it.
    // Re-issue the update on each attempt (after toggling, so the write is
    // never a no-op) and re-sync both sides until it lands on alix2.
    await vi.waitFor(async () => {
      await dm.updateConsentState("allowed");
      await dm.updateConsentState("denied");
      await alix.preferences.sync();
      await alix2.preferences.sync();
      expect((await dm2.state()).consentState).toBe("denied");
    }, WAIT);

    // update consent back to allowed on alix2
    await alix2.preferences.setConsentStates([
      {
        entity: { kind: "conversation", conversationId: dm2.id },
        state: "allowed",
      },
    ]);

    const convoState = await alix2.preferences.consentState({
      kind: "conversation",
      conversationId: dm2.id,
    });
    expect(convoState).toBe("allowed");

    const updatedConsentState = (await dm2.state()).consentState;
    expect(updatedConsentState).toBe("allowed");
  });

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
