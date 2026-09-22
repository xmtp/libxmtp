import { ConsentState } from "@xmtp/node-bindings";
import { describe, expect, it } from "vitest";

import { toBindingNotificationConfig } from "@/Notifications";

describe("NotificationConfig", () => {
  it.each(["apns", "fcm"] as const)(
    "preserves the %s token and rules at the binding boundary",
    (type) => {
      const token = type === "apns" ? "ab".repeat(32) : "fcm:token-_123";
      expect(
        toBindingNotificationConfig({
          channel: { type, token },
          consentStates: [ConsentState.Denied, ConsentState.Unknown],
          includeWelcomes: false,
          includeSyncGroups: true,
          includeCommits: true,
        }),
      ).toEqual({
        channel: type,
        token,
        consentStates: [ConsentState.Denied, ConsentState.Unknown],
        includeWelcomes: false,
        includeSyncGroups: true,
        includeCommits: true,
      });
      expect(toBindingNotificationConfig({ channel: { type, token } })).toEqual(
        { channel: type, token },
      );
    },
  );
});
