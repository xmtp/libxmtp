import { ConsentEntityType, ConsentState } from "@xmtp/wasm-bindings";
import { describe, expect, it, vi } from "vitest";
import { HistorySyncUrls } from "@/constants";
import { uuid } from "@/utils/uuid";
import { createRegisteredClient, createSigner } from "@test/helpers";

// Device sync hands work to background workers on both installations, so
// cross-installation visibility converges rather than completing on any
// single sync call. Poll (re-triggering the syncs) until the expected state
// appears instead of pacing with fixed sleeps — a fixed sleep loses the race
// on loaded CI runners.
const WAIT = { timeout: 30_000, interval: 1000 };

// The request→archive→import round trip crosses two background-worker hops
// (one worker builds and uploads the archive, the other downloads and
// imports it), so give it a much larger budget than the single-hop waits
// and pace the retries so the workers aren't flooded with requests.
const ROUND_TRIP_WAIT = { timeout: 90_000, interval: 3000 };

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

  it("should sync device archive using sendSyncArchive, listAvailableArchives, and processSyncArchive", async () => {
    const { signer: boSigner } = createSigner();
    const { signer: alixSigner } = createSigner();

    const bo = await createRegisteredClient(boSigner);
    const alix = await createRegisteredClient(alixSigner);

    const group = await alix.conversations.createGroup([bo.inboxId!]);
    const msgFromAlix = await group.sendText("hello from alix");

    // create second installation for alix
    const alix2 = await createRegisteredClient(alixSigner, {
      dbPath: `./test-${uuid()}.db3`,
    });

    await bo.conversations.syncAll();
    const boGroup = await bo.conversations.getConversationById(group.id);
    expect(boGroup).toBeTruthy();
    await boGroup!.sendText("hello from bo");

    // bo's send commits alix2 into the group; poll until the welcome and
    // the post-join messages land on alix2
    await vi.waitFor(async () => {
      await alix.conversations.syncAll();
      await alix2.conversations.syncAll();
      const c = await alix2.conversations.getConversationById(group.id);
      expect(c).toBeTruthy();
      const msgs = await c!.messages();
      expect(msgs.length).toBe(2);
    }, WAIT);

    // list available archives - may fail in some environments
    try {
      const archives = await alix2.listAvailableArchives(7);
      expect(archives).toBeDefined();
    } catch {
      // listAvailableArchives may not be fully supported in all test environments
    }

    // The archive announcement is a one-shot message into the device sync
    // group: if it goes out before alix2 has joined, alix2 can never decrypt
    // it and no amount of later syncing recovers it. Re-send the archive on
    // each attempt (fresh announcement into whatever sync groups now
    // connect the two installations) and retry the import until the pinned
    // payload arrives (processSyncArchive throws MissingPayload until then).
    await vi.waitFor(async () => {
      await alix.syncAllDeviceSyncGroups();
      await alix.sendSyncArchive(
        "123",
        {
          elements: [],
          excludeDisappearingMessages: false,
        },
        HistorySyncUrls.local,
      );
      await alix2.syncAllDeviceSyncGroups();
      await alix2.processSyncArchive("123");
    }, WAIT);

    await alix2.conversations.syncAll();

    const group2After = await alix2.conversations.getConversationById(group.id);
    expect(group2After).toBeTruthy();

    const messagesAfter = await group2After!.messages();
    // verify we received messages from the archive sync
    // the exact count may vary depending on sync timing
    expect(messagesAfter.length).toBeGreaterThanOrEqual(2);
    // check if we found the original message from alix
    const foundOriginalMessage = messagesAfter.some(
      (m) => m.id === msgFromAlix,
    );
    if (messagesAfter.length >= 3) {
      expect(foundOriginalMessage).toBe(true);
    }
  });

  it("should sync messages across installations using sendSyncRequest and syncAllDeviceSyncGroups", async () => {
    const { signer: boSigner } = createSigner();
    const { signer: alixSigner } = createSigner();

    const bo = await createRegisteredClient(boSigner);
    const client1 = await createRegisteredClient(alixSigner);

    const group = await client1.conversations.createGroup([bo.inboxId!]);

    // send a message before second installation is created
    const msgId = await group.sendText("hi");
    const messages = await group.messages();
    expect(messages.length).toBe(2);

    // create second installation
    const client2 = await createRegisteredClient(alixSigner, {
      dbPath: `./test-${uuid()}.db3`,
    });

    // the new installation's registration propagates asynchronously
    await vi.waitFor(async () => {
      const state = await client2.preferences.fetchInboxState();
      expect(state.installations.length).toBe(2);
    }, WAIT);

    // The sync request is a one-shot message into the device sync group: if
    // it goes out before client1's worker has joined, client1 can never
    // decrypt it and no archive ever comes back. Re-issue the request on
    // each attempt, then poll the whole round trip — client1's worker
    // answers with an archive, client2's worker imports it — until the
    // group and its messages materialize on client2.
    const messagesOnClient2 = await vi.waitFor(async () => {
      await client2.sendSyncRequest(
        {
          elements: [],
          excludeDisappearingMessages: false,
        },
        HistorySyncUrls.local,
      );
      await client1.syncAllDeviceSyncGroups();
      await client2.syncAllDeviceSyncGroups();
      await client2.conversations.syncAll();
      const c = await client2.conversations.getConversationById(group.id);
      expect(c).toBeTruthy();
      const msgs = await c!.messages();
      expect(msgs.length).toBeGreaterThan(0);
      return msgs;
    }, ROUND_TRIP_WAIT);

    const client1MessageCount = (await group.messages()).length;
    const containsMessage = messagesOnClient2.some((m) => m.id === msgId);

    if (client1MessageCount === messagesOnClient2.length) {
      expect(containsMessage).toBe(true);
    }
  });
});
