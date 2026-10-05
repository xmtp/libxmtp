import {
  createClient,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
import { type ClientEvent, type ConsentRecord } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  currentProjection,
  lowerEventFilter,
} from "../dist/public-values.gen.js";

const WAIT = { timeout: 30_000, interval: 100 };
describe("Preferences", () => {
  it("defaults the omitted own-message event filter before binding conversion", () => {
    const projection = currentProjection();
    expect(
      lowerEventFilter({ kinds: ["conversation.joined"] }, projection)
        .referencesOwnMessages,
    ).toBe(false);
    for (const selected of [false, true]) {
      expect(
        lowerEventFilter(
          { kinds: ["message.received"], references_own_messages: selected },
          projection,
        ).referencesOwnMessages,
      ).toBe(selected);
    }
  });
  it("uses byte group IDs in the named event payload", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const events = await client.events({
      kinds: ["conversation.joined"],
    });
    try {
      const group = await client.conversations.createGroup([]);
      const next = await events.next();
      expect(next.done).toBe(false);
      if (next.done || next.value.kind !== "conversation.joined")
        throw new Error("expected a joined event");
      const payload = next.value.conversation_joined;
      expect(payload.group_id).toBeInstanceOf(Uint8Array);
      expect(Buffer.from(payload.group_id)).toEqual(
        Buffer.from(group.id, "hex"),
      );
      expect(payload.conversation_type).toBe("group");
      expect(payload.origin).toBe("created");
      expect("conversationId" in payload).toBe(false);
    } finally {
      await events.return();
      await client.end();
    }
  });
  it("reads local and remote inbox identity states", async () => {
    const { signer, identifier } = createSigner();
    const client = await createRegisteredClient(signer);
    const local = await client.inboxState(false);
    expect(local.inboxId).toBe(client.inboxId);
    expect(local.installations.map((i) => i.id)).toEqual([
      client.installationId,
    ]);
    expect(local.identities).toEqual([identifier]);
    expect(local.recoveryIdentity).toEqual(identifier);
    const other = await createClient(createSigner().signer);
    const [remote] = await other.inboxStates([client.inboxId], true);
    expect(remote).toEqual(local);
  });
  it("reads each selected inbox", async () => {
    const first = await createRegisteredClient(createSigner().signer);
    const second = await createRegisteredClient(createSigner().signer);
    const states = await first.inboxStates(
      [first.inboxId, second.inboxId],
      true,
    );
    expect(states.map((s) => s.inboxId).sort()).toEqual(
      [first.inboxId, second.inboxId].sort(),
    );
    expect(
      states.find((s) => s.inboxId === second.inboxId)?.identities,
    ).toEqual([second.identity]);
  });
  it("shares consent between preferences and the conversation state", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const group = await client.conversations.createGroup([]);
    const entity = { kind: "conversation" as const, conversationId: group.id };
    await client.preferences.setConsentStates([{ entity, state: "allowed" }]);
    expect(await client.preferences.consentState(entity)).toBe("allowed");
    expect((await group.state()).common.consentState).toBe("allowed");
    await group.updateConsentState("denied");
    expect(await client.preferences.consentState(entity)).toBe("denied");
  });
  it("delivers every consent change from a multi-record update", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const peer = await createRegisteredClient(createSigner().signer);
    const group = await client.conversations.createGroup([peer.inboxId]);
    const events = await client.events({
      kinds: ["consent.changed"],
      references_own_messages: false,
    });
    const seen: ClientEvent[] = [];
    const consumed = (async () => {
      for await (const event of events) seen.push(event);
    })();
    try {
      await group.updateConsentState("denied");
      await vi.waitFor(
        () =>
          expect(seen).toContainEqual({
            kind: "consent.changed",
            consent_changed: {
              entity_kind: "conversation",
              entity: group.id,
              state: "denied",
            },
          }),
        WAIT,
      );
      const records: ConsentRecord[] = [
        {
          entity: { kind: "conversation", conversationId: group.id },
          state: "allowed",
        },
        { entity: { kind: "inbox", inboxId: peer.inboxId }, state: "denied" },
      ];
      await client.preferences.setConsentStates(records);
      await vi.waitFor(() => {
        expect(seen).toContainEqual({
          kind: "consent.changed",
          consent_changed: {
            entity_kind: "conversation",
            entity: group.id,
            state: "allowed",
          },
        });
        expect(seen).toContainEqual({
          kind: "consent.changed",
          consent_changed: {
            entity_kind: "inbox",
            entity: peer.inboxId,
            state: "denied",
          },
        });
      }, WAIT);
    } finally {
      await events.return();
      await consumed;
    }
  });
  it("reports HMAC updates from new installations and exposes current keys", async () => {
    const { signer } = createSigner();
    const client = await createRegisteredClient(signer, { deviceSync: true });
    const peer = await createRegisteredClient(createSigner().signer);
    const group = await client.conversations.createGroup([peer.inboxId]);
    const events = await client.events({
      kinds: ["consent.changed", "hmac_keys.updated"],
      references_own_messages: false,
    });
    const seen: ClientEvent[] = [];
    const consumed = (async () => {
      for await (const event of events) seen.push(event);
    })();
    try {
      await group.updateConsentState("denied");
      const second = await createRegisteredClient(signer, { deviceSync: true });
      const third = await createRegisteredClient(signer, { deviceSync: true });
      await vi.waitFor(async () => {
        await client.conversations.syncAll(undefined);
        await second.conversations.syncAll(undefined);
        await third.conversations.syncAll(undefined);
        expect(seen.some((e) => e.kind === "hmac_keys.updated")).toBe(true);
      }, WAIT);
      expect(seen).toContainEqual({
        kind: "consent.changed",
        consent_changed: {
          entity_kind: "conversation",
          entity: group.id,
          state: "denied",
        },
      });
      await group.updateConsentState("allowed");
      const keys = await client.conversations.hmacKeys();
      expect(keys.get(group.id)?.length).toBeGreaterThan(0);
      for (const key of keys.get(group.id)!)
        expect(key.key).toBeInstanceOf(Uint8Array);
    } finally {
      await events.return();
      await consumed;
    }
  });
});
