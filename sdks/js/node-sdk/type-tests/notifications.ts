import type {
  Client,
  Conversation,
  Dm,
  Group,
  NotificationError,
  NotificationChannel,
  NotificationConfig,
  NotificationOverride,
  NotificationState,
} from "@xmtp/node-sdk";

export const channels: NotificationChannel[] = [
  { type: "apns", token: "token" },
  { type: "fcm", token: "token" },
  {
    type: "http",
    url: "https://example.com/push",
    signingKey: new Uint8Array(32),
  },
];

// @ts-expect-error APNs requires a token.
export const missingToken: NotificationChannel = { type: "apns" };
// @ts-expect-error HTTPS requires a byte array, not a number array.
export const wrongBytes: NotificationChannel = {
  type: "http",
  url: "https://example.com",
  signingKey: [1],
};
// @ts-expect-error Fields from another channel are not accepted.
export const mixedChannel: NotificationChannel = {
  type: "fcm",
  token: "token",
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
    metadata: new Uint8Array([0, 255]),
  };
  const enabled: NotificationState = await client.enableNotifications(config);
  const state: NotificationState = await client.notificationState();
  if (state.state === "failed") {
    const error: NotificationError = state.error;
    const code: string = error.code;
    void code;
  }
  const value: NotificationOverride = "default";
  await conversation.setNotifications(value);
  const effective: boolean = await conversation.notificationsEnabled();
  const disabled: Promise<void> = client.disableNotifications();
  await disabled;
  void [enabled, effective];
}
