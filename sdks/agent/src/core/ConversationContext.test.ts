import type { Client, Conversation } from "@xmtp/node-sdk";
import { describe, expect, expectTypeOf, it } from "vitest";

import { ConversationContext } from "./ConversationContext";

describe("ConversationContext consent methods", () => {
  for (const consentState of ["allowed", "denied", "unknown"] as const) {
    it(`reads ${consentState} from the conversation state`, async () => {
      const conversation = {
        state: async () => ({ common: { consentState } }),
      } as unknown as Conversation;
      const context = new ConversationContext({
        conversation,
        client: {} as Client,
      });

      expectTypeOf(context.isAllowed).toEqualTypeOf<() => Promise<boolean>>();
      expectTypeOf(context.isDenied).toEqualTypeOf<() => Promise<boolean>>();
      expectTypeOf(context.isUnknown).toEqualTypeOf<() => Promise<boolean>>();
      expect(await context.isAllowed()).toBe(consentState === "allowed");
      expect(await context.isDenied()).toBe(consentState === "denied");
      expect(await context.isUnknown()).toBe(consentState === "unknown");
    });
  }
});
