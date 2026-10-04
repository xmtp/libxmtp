import type { Client, Conversation } from "@xmtp/node-sdk";
import { describe, expect, expectTypeOf, it, vi } from "vitest";

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

describe("ConversationContext remote attachments", () => {
  it("uses the SDK push default", async () => {
    const remoteAttachment = { url: "https://example.com/attachment" };
    const sendRemoteAttachment = vi.fn().mockResolvedValue(undefined);
    const upload = vi.fn().mockResolvedValue(undefined);
    const create = vi.fn().mockResolvedValue({
      upload,
      remoteAttachment,
    });
    const conversation = { sendRemoteAttachment } as unknown as Conversation;
    const client = { attachments: { create } } as unknown as Client;
    const context = new ConversationContext({ conversation, client });

    await context.sendRemoteAttachment(new File(["hello"], "hello.txt"));

    expect(upload).toHaveBeenCalledOnce();
    expect(sendRemoteAttachment).toHaveBeenCalledExactlyOnceWith(
      remoteAttachment,
    );
  });
});
