import { ConsentEntityType, ConsentState } from "@xmtp/wasm-bindings";
import { describe, expect, it, vi } from "vitest";
import { uuid } from "@/utils/uuid";
import { createRegisteredClient, createSigner } from "@test/helpers";

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

    const bo = await createRegisteredClient(boSigner);
    const alix = await createRegisteredClient(alixSigner);

    // create DM conversation
    const dm = await alix.conversations.createDm(bo.inboxId!);
    const initialConsent = await dm.consentState();
    expect(
      initialConsent === ConsentState.Unknown ||
        initialConsent === ConsentState.Allowed,
    ).toBe(true);

    await bo.conversations.sync();

    // create second installation for alix
    const alix2 = await createRegisteredClient(alixSigner, {
      dbPath: `./test-${uuid()}.db3`,
    });

    // the new installation's registration propagates asynchronously
    await vi.waitFor(async () => {
      const state = await alix2.preferences.fetchInboxState();
      expect(state.installations.length).toBe(2);
    }, WAIT);

    // sync the DM on alix so conversation is pushed
    await dm.sync();
    await alix.conversations.syncAll();

    // alix2 syncs until it has the DM; re-trigger the sender side too
    const dm2 = await vi.waitFor(async () => {
      await alix.conversations.syncAll();
      await alix2.conversations.sync();
      const c = await alix2.conversations.getConversationById(dm.id);
      expect(c).toBeTruthy();
      return c!;
    }, WAIT);

    const consentOnAlix2Before = await dm2.consentState();
    expect(
      consentOnAlix2Before === ConsentState.Unknown ||
        consentOnAlix2Before === ConsentState.Allowed,
    ).toBe(true);

    // update consent to denied on alix
    await dm.updateConsentState(ConsentState.Denied);
    const consentState = await dm.consentState();
    expect(consentState).toBe(ConsentState.Denied);

    await alix.preferences.sync();

    // The consent update is published into the device sync group once — if
    // that happens before alix2 has joined, alix2 can never decrypt it.
    // Re-issue the update on each attempt (after toggling, so the write is
    // never a no-op) and re-sync both sides until it lands on alix2.
    await vi.waitFor(async () => {
      await dm.updateConsentState(ConsentState.Allowed);
      await dm.updateConsentState(ConsentState.Denied);
      await alix.preferences.sync();
      await alix2.preferences.sync();
      expect(await dm2.consentState()).toBe(ConsentState.Denied);
    }, WAIT);

    // update consent back to allowed on alix2
    await alix2.preferences.setConsentStates([
      {
        entityType: ConsentEntityType.GroupId,
        entity: dm2.id,
        state: ConsentState.Allowed,
      },
    ]);

    const convoState = await alix2.preferences.getConsentState(
      ConsentEntityType.GroupId,
      dm2.id,
    );
    expect(convoState).toBe(ConsentState.Allowed);

    const updatedConsentState = await dm2.consentState();
    expect(updatedConsentState).toBe(ConsentState.Allowed);
  });

  it("should export and import a local archive", async () => {
    const { signer: boSigner } = createSigner();
    const { signer: alixSigner } = createSigner();
    const bo = await createRegisteredClient(boSigner);
    const alix = await createRegisteredClient(alixSigner);
    const group = await alix.conversations.createGroup([bo.inboxId!]);
    const message = await group.sendText("archive me");
    const key = new Uint8Array(32).fill(1);
    const archive = await alix.createArchive(key);
    const metadata = await alix.archiveMetadata(archive, key);
    expect(metadata.elements.length).toBeGreaterThan(0);

    const alix2 = await createRegisteredClient(alixSigner, {
      dbPath: `./test-${uuid()}.db3`,
    });
    await alix2.importArchive(archive, key);

    const restored = await alix2.conversations.getConversationById(group.id);
    expect(restored).toBeTruthy();
    expect(
      (await restored!.messages()).some((entry) => entry.id === message),
    ).toBe(true);
  });
});
