import type {
  Client,
  Conversation,
  Group,
  NotificationFailure,
  NotificationChannel,
  NotificationConfig,
  NotificationOverride,
  NotificationState,
} from "@xmtp/node-sdk";
import { Dm } from "@xmtp/node-sdk";

export const channels: NotificationChannel[] = [
  { kind: "apns", token: "token" },
  { kind: "fcm", token: "token" },
  {
    kind: "http",
    url: "https://example.com/push",
    signingKey: new Uint8Array(32),
  },
];

// @ts-expect-error APNs requires a token.
export const missingToken: NotificationChannel = { kind: "apns" };
export const wrongBytes: NotificationChannel = {
  kind: "http",
  url: "https://example.com",
  // @ts-expect-error HTTPS requires a byte array, not a number array.
  signingKey: [1],
};
export const mixedChannel: NotificationChannel = {
  kind: "fcm",
  token: "token",
  // @ts-expect-error Fields from another channel are not accepted.
  url: "https://example.com",
};
// @ts-expect-error The reset value is default.
export const wrongOverride: NotificationOverride = "none";

export async function checkNotifications(
  client: Client,
  conversation: Conversation | Group | Dm,
): Promise<void> {
  const config: NotificationConfig = {
    channel: channels[0],
  };
  const _enabled: NotificationState = await client.enableNotifications(config);
  const state: NotificationState = client.notificationState();
  if (state.kind === "failed") {
    const error: NotificationFailure = state.error;
    const _code: string = error;
  }
  const value: NotificationOverride = "default";
  await conversation.setNotifications(value);
  const _effective: boolean =
    conversation instanceof Dm
      ? (await conversation.state()).notificationsEnabled
      : (await conversation.state()).common.notificationsEnabled;
  const disabled: Promise<void> = client.disableNotifications();
  await disabled;
}
