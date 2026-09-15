import { randomBytes } from "node:crypto";
import { describe, expect, it } from "vitest";
import {
  ConsentState,
  NotificationError,
  WorkerKind,
  type NotificationConfig,
} from "@/index";
import { createRegisteredClient, createSigner } from "@test/helpers";

const httpConfig = (): NotificationConfig => ({
  channel: {
    type: "http",
    url: "https://example.com/xmtp-notification-test",
    signingKey: new Uint8Array(randomBytes(32)),
  },
});

describe("Notifications", () => {
  it("registers HTTP delivery and resets group and DM overrides", async () => {
    const client = await createRegisteredClient(createSigner().signer);
    const peer = await createRegisteredClient(createSigner().signer);
    try {
      const group = await client.conversations.createGroup([peer.inboxId]);
      const dm = await client.conversations.createDm(peer.inboxId);
      expect(await client.notificationState()).toEqual({ state: "disabled" });
      expect(await client.enableNotifications(httpConfig())).toEqual({
        state: "enabled",
      });
      expect(await client.notificationState()).toEqual({ state: "enabled" });

      for (const conversation of [group, dm]) {
        expect(await conversation.notificationsEnabled()).toBe(true);
        await conversation.setNotifications("disabled");
        expect(await conversation.notificationsEnabled()).toBe(false);
        await conversation.setNotifications("default");
        expect(await conversation.notificationsEnabled()).toBe(true);
      }

      await client.enableNotifications({
        ...httpConfig(),
        consentStates: [],
        includeWelcomes: false,
        includeSyncGroups: false,
        includeCommits: true,
        metadata: new Uint8Array([0, 128, 255]),
      });
      for (const conversation of [group, dm]) {
        expect(await conversation.notificationsEnabled()).toBe(false);
        await conversation.setNotifications("enabled");
        expect(await conversation.notificationsEnabled()).toBe(true);
        await conversation.setNotifications("default");
        expect(await conversation.notificationsEnabled()).toBe(false);
      }
      await client.disableNotifications();
      expect(await client.notificationState()).toEqual({ state: "disabled" });
    } finally {
      await client.close();
      await peer.close();
    }
  });

  it.each(["apns", "fcm"] as const)(
    "returns a typed failure for an unconfigured %s channel",
    async (type) => {
      const client = await createRegisteredClient(createSigner().signer);
      try {
        await expect(
          client.enableNotifications({
            channel: { type, token: "a".repeat(64) },
            consentStates: [ConsentState.Allowed],
          }),
        ).rejects.toMatchObject({
          name: "NotificationError",
          code: "NotificationError::ChannelNotConfigured",
        });
        const state = await client.notificationState();
        expect(state.state).toBe("failed");
        if (state.state !== "failed") throw new Error("Expected failed state");
        expect(state.error).toBeInstanceOf(NotificationError);
        expect(state.error.code).toBe(
          "NotificationError::ChannelNotConfigured",
        );
        await client.disableNotifications();
        expect(await client.notificationState()).toEqual({ state: "disabled" });
      } finally {
        await client.close();
      }
    },
  );

  it("returns a typed task-runner failure without enabling notifications", async () => {
    const client = await createRegisteredClient(createSigner().signer, {
      workerConfig: { disabledWorkers: [WorkerKind.TaskRunner] },
    });
    try {
      await expect(
        client.enableNotifications(httpConfig()),
      ).rejects.toMatchObject({
        name: "NotificationError",
        code: "NotificationError::TaskRunnerDisabled",
      });
      expect(await client.notificationState()).toEqual({ state: "disabled" });
    } finally {
      await client.close();
    }
  });
});
