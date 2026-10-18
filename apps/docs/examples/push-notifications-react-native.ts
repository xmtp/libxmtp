import type {
  Client,
  Conversation,
  NotificationChannel,
} from "@xmtp/react-native-sdk";

export async function configureNotifications(
  client: Client,
  conversation: Conversation,
  token: string,
  platform: "ios" | "android",
) {
  // #region configure
  const channel: NotificationChannel = {
    kind: platform === "ios" ? "apns" : "fcm",
    token,
  };
  await client.enableNotifications({ channel });
  const state = client.notificationState();
  if (state.kind === "failed") {
    throw new Error(`Notification registration failed: ${state.error}`);
  }
  await conversation.setNotifications("disabled");
  await conversation.setNotifications("default");
  // #endregion configure
}

export async function receivePushHint(
  client: Client,
  hint: { topic: string; sequence_id: string },
  decodeBase64: (value: string) => Uint8Array,
) {
  // #region receive
  // The app supplies a strict standard-base64 decoder.
  const topic = decodeBase64(hint.topic);
  if (!/^[0-9]+$/.test(hint.sequence_id)) {
    throw new Error("Invalid push sequence");
  }
  const sequenceId = BigInt(hint.sequence_id);
  if (sequenceId === 0n || sequenceId > 18446744073709551615n) {
    throw new Error("Invalid push sequence");
  }
  const isGroup = topic[0] === 0 && topic.length === 17;
  const isWelcome = topic[0] === 1 && topic.length === 33;
  if (!isGroup && !isWelcome) throw new Error("Invalid push topic");

  if (isWelcome) {
    await client.conversations.sync();
    return;
  }
  const groupId = Array.from(topic.subarray(1), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  let conversation = await client.conversations.getById(groupId);
  if (!conversation) {
    await client.conversations.sync();
    conversation = await client.conversations.getById(groupId);
  }
  if (!conversation) throw new Error("Conversation is not available yet");
  await conversation.sync();
  const messages = await conversation.messages();
  // #endregion receive
  return { sequenceId, messages };
}
