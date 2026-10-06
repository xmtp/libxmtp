import { createRegisteredClient, createSigner } from "@test/helpers";
import {
  type ClientEvent,
  type ConsentRecord,
  generateInboxId,
} from "@xmtp/node-sdk";
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

  it("uses byte group IDs in the named event payload and ends a pending read", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const events = await client.events({
      kinds: ["conversation.joined"],
    });
    const seen: ClientEvent[] = [];
    const consumed = (async () => {
      for await (const event of events) seen.push(event);
    })();
    try {
      const group = await client.conversations.createGroup([]);
      await vi.waitFor(() => expect(seen.length).toBeGreaterThan(0), {
        timeout: 30_000,
        interval: 100,
      });
      const [event] = seen;
      if (event?.kind !== "conversation.joined")
        throw new Error("expected a joined event");
      const payload = event.conversation_joined;
      expect(payload.group_id).toBeInstanceOf(Uint8Array);
      expect(Buffer.from(payload.group_id)).toEqual(
        Buffer.from(group.id, "hex"),
      );
      expect(payload.conversation_type).toBe("group");
      expect(payload.origin).toBe("created");
      expect("conversationId" in payload).toBe(false);
      // The loop is now waiting in next(). return() must abort that read and
      // end the loop without an error.
      await events.return();
      await expect(consumed).resolves.toBeUndefined();
    } finally {
      await events.return();
      await client.end();
    }
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

  // verifies: EVENT-009
  it("delivers every consent change from a multi-record update", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const group = await client.conversations.createGroup([]);
    const peerInboxId = generateInboxId(createSigner().identifier);
    const events = await client.events({ kinds: ["consent.changed"] });
    const seen: ClientEvent[] = [];
    const consumed = (async () => {
      for await (const event of events) seen.push(event);
    })();
    try {
      const records: ConsentRecord[] = [
        {
          entity: { kind: "conversation", conversationId: group.id },
          state: "denied",
        },
        { entity: { kind: "inbox", inboxId: peerInboxId }, state: "allowed" },
      ];
      await client.preferences.setConsentStates(records);
      await vi.waitFor(() => {
        expect(seen).toContainEqual({
          kind: "consent.changed",
          consent_changed: {
            entity_kind: "conversation",
            entity: group.id,
            state: "denied",
          },
        });
        expect(seen).toContainEqual({
          kind: "consent.changed",
          consent_changed: {
            entity_kind: "inbox",
            entity: peerInboxId,
            state: "allowed",
          },
        });
      }, WAIT);
    } finally {
      // The loop still waits in next(); return() must end it.
      await events.return();
      await consumed;
      await client.end();
    }
  });
});
