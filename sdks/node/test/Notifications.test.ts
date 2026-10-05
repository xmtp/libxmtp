import { randomBytes } from "node:crypto";

import { createRegisteredClient, createSigner } from "@test/helpers";
import { Dm, type Group, type Conversation } from "@xmtp/node-sdk";
import { type NotificationConfig } from "@xmtp/node-sdk";
import { describe, expect, it } from "vitest";

const httpConfig = (): NotificationConfig => ({
  channel: {
    kind: "http",
    url: "https://example.com/xmtp-notification-test",
    signingKey: new Uint8Array(randomBytes(32)),
  },
});

const notificationEnabled = async (conversation: Conversation) => {
  const state = await conversation.state();
  return conversation instanceof Dm
    ? (state as Awaited<ReturnType<Dm["state"]>>).notificationsEnabled
    : (state as Awaited<ReturnType<Group["state"]>>).common
        .notificationsEnabled;
};

describe("Notifications", () => {
  it("registers HTTP delivery and resets group and DM overrides", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const peer = await createRegisteredClient(createSigner().signer);
    try {
      const group = await client.conversations.createGroup([peer.inboxId]);
      const dm = await client.conversations.createDm(peer.inboxId);
      expect(client.notificationState()).toEqual({ kind: "disabled" });
      expect(await client.enableNotifications(httpConfig())).toEqual({
        kind: "enabled",
      });
      expect(client.notificationState()).toEqual({ kind: "enabled" });

      for (const conversation of [group, dm]) {
        expect(await notificationEnabled(conversation)).toBe(true);
        await conversation.setNotifications("disabled");
        expect(await notificationEnabled(conversation)).toBe(false);
        await conversation.setNotifications("default");
        expect(await notificationEnabled(conversation)).toBe(true);
      }

      await client.enableNotifications({
        ...httpConfig(),
        consentStates: [],
        includeWelcomes: false,
        includeSyncGroups: false,
        includeCommits: true,
      });
      for (const conversation of [group, dm]) {
        expect(await notificationEnabled(conversation)).toBe(false);
        await conversation.setNotifications("enabled");
        expect(await notificationEnabled(conversation)).toBe(true);
        await conversation.setNotifications("default");
        expect(await notificationEnabled(conversation)).toBe(false);
      }
      await client.disableNotifications();
      expect(client.notificationState()).toEqual({ kind: "disabled" });
    } finally {
      await client.end();
      await peer.end();
    }
  });
});
