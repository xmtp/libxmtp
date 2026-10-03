import { randomBytes } from "node:crypto";

import {
  clientOptions,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
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
  it("restores notification state, rules, and overrides after restart", async () => {
    const signer = createSigner().signer;
    const { storage } = clientOptions();
    let client = await createRegisteredClient(signer, { storage });
    const peer = await createRegisteredClient(createSigner().signer);
    try {
      const group = await client.conversations.createGroup([peer.inboxId]);
      const groupId = group.id;
      const installationId = client.installationId;
      await client.enableNotifications({
        ...httpConfig(),
        consentStates: [],
        includeWelcomes: false,
      });
      await group.setNotifications("enabled");
      await client.end();
      client = await createRegisteredClient(signer, { storage });
      expect(client.installationId).toBe(installationId);
      expect(client.notificationState()).toEqual({ kind: "enabled" });
      const restored = await client.conversations.getById(groupId);
      expect(restored).toBeDefined();
      if (!restored) throw new Error("Expected the saved group");
      expect(await notificationEnabled(restored)).toBe(true);
      await restored.setNotifications("default");
      expect(await notificationEnabled(restored)).toBe(false);
      await client.disableNotifications();
      await client.end();
      client = await createRegisteredClient(signer, { storage });
      expect(client.installationId).toBe(installationId);
      expect(client.notificationState()).toEqual({ kind: "disabled" });
    } finally {
      await client.end();
      await peer.end();
    }
  });

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

  it.each(["apns", "fcm"] as const)(
    "returns a typed failure for an unconfigured %s channel",
    async (type) => {
      const client = await createRegisteredClient(createSigner().signer);
      try {
        await expect(
          client.enableNotifications({
            channel: { kind: type, token: "a".repeat(64) },
            consentStates: ["allowed"],
          }),
        ).rejects.toMatchObject({
          details: { code: "ChannelNotConfigured" },
        });
        const state = client.notificationState();
        expect(state.kind).toBe("failed");
        if (state.kind !== "failed") throw new Error("Expected failed state");
        expect(state.error).toBe("channelNotConfigured");
        await client.disableNotifications();
        expect(client.notificationState()).toEqual({ kind: "disabled" });
      } finally {
        await client.end();
      }
    },
  );

  it("returns a typed task-runner failure without enabling notifications", async () => {
    const client = await createRegisteredClient(createSigner().signer, {
      workers: { intervals: [{ kind: "taskRunner", enabled: false }] },
    });
    try {
      await expect(
        client.enableNotifications(httpConfig()),
      ).rejects.toMatchObject({
        details: { code: "TaskRunnerDisabled" },
      });
      expect(client.notificationState()).toEqual({ kind: "disabled" });
    } finally {
      await client.end();
    }
  });
});
