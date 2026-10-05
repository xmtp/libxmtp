import { createRegisteredClient, createSigner } from "@test/helpers";
import { describe, expect, it } from "vitest";

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
