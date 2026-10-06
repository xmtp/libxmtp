import { createRegisteredClient, createSigner } from "@test/helpers";
import type { ClientEvent } from "@xmtp/node-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  currentProjection,
  lowerEventFilter,
} from "../dist/public-values.gen.js";

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
});
