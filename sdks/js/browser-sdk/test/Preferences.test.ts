import {
  ConsentEntityType,
  ConsentState,
  type Consent,
  type UserPreferenceUpdate,
} from "@xmtp/wasm-bindings";
import { describe, expect, it, vi } from "vitest";
import { uuid } from "@/utils/uuid";
import {
  createClient,
  createRegisteredClient,
  createSigner,
  waitFor,
} from "@test/helpers";

// Preference updates propagate through background sync-group workers;
// poll until the expected state appears instead of pacing with fixed
// sleeps — a fixed sleep loses the race on loaded CI runners.
const WAIT = { timeout: 30_000, interval: 1000 };

describe("Preferences", () => {
  it("should return the correct inbox state", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer);
    const inboxState = await client.preferences.inboxState();
    expect(inboxState.inboxId).toBe(client.inboxId);
    expect(inboxState.installations.map((install) => install.id)).toEqual([
      client.installationId,
    ]);
    expect(inboxState.accountIdentifiers).toEqual([
      await signer.getIdentifier(),
    ]);
    expect(inboxState.recoveryIdentifier).toStrictEqual(
      await signer.getIdentifier(),
    );

    const { signer: signer2 } = createSigner();
    const client2 = await createClient(signer2);
    const inboxStates = await client2.preferences.fetchInboxStates([
      client.inboxId!,
    ]);
    const inboxState2 = inboxStates[0];
    expect(inboxState2.inboxId).toBe(client.inboxId);
    expect(inboxState.installations.length).toBe(1);
    expect(inboxState.installations[0].id).toBe(client.installationId);
    expect(inboxState2.accountIdentifiers).toEqual([
      await signer.getIdentifier(),
    ]);
    expect(inboxState2.recoveryIdentifier).toStrictEqual(
      await signer.getIdentifier(),
    );
  });

  it("should get inbox states from inbox IDs", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer2);
    const inboxStates = await client.preferences.getInboxStates([
      client.inboxId!,
    ]);
    expect(inboxStates.length).toBe(1);
    expect(inboxStates[0].inboxId).toBe(client.inboxId);
    expect(inboxStates[0].accountIdentifiers).toEqual([
      await signer.getIdentifier(),
    ]);

    const inboxStates2 = await client2.preferences.fetchInboxStates([
      client2.inboxId!,
    ]);
    expect(inboxStates2.length).toBe(1);
    expect(inboxStates2[0].inboxId).toBe(client2.inboxId);
    expect(inboxStates2[0].accountIdentifiers).toEqual([
      await signer2.getIdentifier(),
    ]);
  });

  it("should manage consent states", async () => {
    const { signer: signer1 } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer1);
    const client2 = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([client2.inboxId!]);

    await client2.conversations.sync();
    const group2 = await client2.conversations.getConversationById(group.id);

    expect(group2).not.toBeNull();

    expect(
      await client2.preferences.getConsentState(
        ConsentEntityType.GroupId,
        group2!.id,
      ),
    ).toBe(ConsentState.Unknown);

    await client2.preferences.setConsentStates([
      {
        entityType: ConsentEntityType.GroupId,
        entity: group2!.id,
        state: ConsentState.Allowed,
      },
    ]);

    expect(
      await client2.preferences.getConsentState(
        ConsentEntityType.GroupId,
        group2!.id,
      ),
    ).toBe(ConsentState.Allowed);

    expect(await group2!.consentState()).toBe(ConsentState.Allowed);

    await group2!.updateConsentState(ConsentState.Denied);

    expect(
      await client2.preferences.getConsentState(
        ConsentEntityType.GroupId,
        group2!.id,
      ),
    ).toBe(ConsentState.Denied);
  });

  it("should stream consent updates", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client = await createRegisteredClient(signer);
    const client2 = await createRegisteredClient(signer2);
    const group = await client.conversations.createGroup([client2.inboxId!]);
    const stream = await client.preferences.streamConsent();

    // Consume the stream in the background. Background workers can emit
    // their own consent batches at any time, so batch counts and indices
    // aren't stable — wait for and assert on the content of the updates
    // we issue instead.
    const batches: Consent[][] = [];
    const consumed = (async () => {
      for await (const updates of stream) {
        batches.push(updates);
      }
    })();

    const observed = (
      entityType: ConsentEntityType,
      entity: string,
      state: ConsentState,
    ) =>
      batches
        .flat()
        .some(
          (u) =>
            u.entityType === entityType &&
            u.entity === entity &&
            u.state === state,
        );

    await group.updateConsentState(ConsentState.Denied);
    await vi.waitFor(
      () =>
        expect(
          observed(ConsentEntityType.GroupId, group.id, ConsentState.Denied),
        ).toBe(true),
      WAIT,
    );

    await client.preferences.setConsentStates([
      {
        entity: group.id,
        entityType: ConsentEntityType.GroupId,
        state: ConsentState.Allowed,
      },
    ]);
    await vi.waitFor(
      () =>
        expect(
          observed(ConsentEntityType.GroupId, group.id, ConsentState.Allowed),
        ).toBe(true),
      WAIT,
    );

    await client.preferences.setConsentStates([
      {
        entity: group.id,
        entityType: ConsentEntityType.GroupId,
        state: ConsentState.Denied,
      },
      {
        entity: client2.inboxId!,
        entityType: ConsentEntityType.InboxId,
        state: ConsentState.Allowed,
      },
    ]);
    // the two-entry update is delivered together in a single batch
    await vi.waitFor(
      () =>
        expect(
          batches.some(
            (b) =>
              b.some(
                (u) =>
                  u.entityType === ConsentEntityType.GroupId &&
                  u.entity === group.id &&
                  u.state === ConsentState.Denied,
              ) &&
              b.some(
                (u) =>
                  u.entityType === ConsentEntityType.InboxId &&
                  u.entity === client2.inboxId &&
                  u.state === ConsentState.Allowed,
              ),
          ),
        ).toBe(true),
      WAIT,
    );

    await stream.end();
    await consumed;
  });

  it("should stream preferences", async () => {
    const { signer } = createSigner();
    const { signer: signer2 } = createSigner();
    const client1 = await createRegisteredClient(signer);
    const clientB = await createRegisteredClient(signer2);
    const group = await client1.conversations.createGroup([clientB.inboxId!]);
    const stream = await client1.preferences.streamPreferences();

    await group.updateConsentState(ConsentState.Denied);
    await client1.preferences.setConsentStates([
      {
        entity: clientB.inboxId!,
        entityType: ConsentEntityType.InboxId,
        state: ConsentState.Denied,
      },
    ]);

    const client2 = await createRegisteredClient(signer, {
      dbPath: `./test-${uuid()}.db3`,
    });

    const client3 = await createRegisteredClient(signer, {
      dbPath: `./test-${uuid()}.db3`,
    });

    await client3.conversations.syncAll();
    await client2.conversations.syncAll();

    // Collect updates concurrently while the preference changes propagate.
    const preferences: UserPreferenceUpdate[] = [];
    const collecting = (async () => {
      for await (const update of stream) {
        preferences.push(...update);
      }
    })();

    // Four updates are expected: two consent updates (the group and the
    // inbox-id consent changes) and two HMAC-key updates (one per new
    // installation). These propagate over the network asynchronously, so
    // re-sync until the stream has observed all of them rather than racing a
    // fixed delay, which can cut off the last update and yield only 3.
    try {
      await waitFor(
        async () => {
          await client1.conversations.syncAll();
          return preferences.length >= 4;
        },
        { timeout: 30000, interval: 1000 },
      );
    } finally {
      await stream.end();
      await collecting;
    }

    expect(preferences.length).toBe(4);
    const consentUpdate1 = preferences[0] as Extract<
      UserPreferenceUpdate,
      { type: "ConsentUpdate" }
    >;
    expect(consentUpdate1.type).toBe("ConsentUpdate");
    expect(consentUpdate1.consent).toEqual({
      entity: group.id,
      entityType: ConsentEntityType.GroupId,
      state: ConsentState.Denied,
    });
    const consentUpdate2 = preferences[1] as Extract<
      UserPreferenceUpdate,
      { type: "ConsentUpdate" }
    >;
    expect(consentUpdate2.type).toBe("ConsentUpdate");
    expect(consentUpdate2.consent).toEqual({
      entity: clientB.inboxId!,
      entityType: ConsentEntityType.InboxId,
      state: ConsentState.Denied,
    });
    const hmacKeyUpdate1 = preferences[2] as Extract<
      UserPreferenceUpdate,
      { type: "HmacKeyUpdate" }
    >;
    expect(hmacKeyUpdate1.type).toBe("HmacKeyUpdate");
    expect(hmacKeyUpdate1.key).toBeInstanceOf(Uint8Array);
    const hmacKeyUpdate2 = preferences[3] as Extract<
      UserPreferenceUpdate,
      { type: "HmacKeyUpdate" }
    >;
    expect(hmacKeyUpdate2.type).toBe("HmacKeyUpdate");
    expect(hmacKeyUpdate2.key).toBeInstanceOf(Uint8Array);
  });
});
