import type { Client, Conversation, NotificationChannel } from "@xmtp/node-sdk";

export async function configureNotifications(
  client: Client,
  conversation: Conversation,
  token: string,
) {
  // #region configure
  await client.enableNotifications({ channel: { type: "fcm", token } });
  await conversation.setNotifications("disabled");
  const enabled = await conversation.notificationsEnabled();
  await conversation.setNotifications("default");
  const state = await client.notificationState();
  if (state.state === "failed") {
    console.error(state.error.code);
  }
  await client.disableNotifications();
  // #endregion configure
  return enabled;
}

export function webhookChannel(url: string, signingKey: Uint8Array) {
  // #region webhook
  const channel: NotificationChannel = { type: "http", url, signingKey };
  // #endregion webhook
  return channel;
}

export async function receivePushHint(
  client: Client,
  hint: { topic: string; sequence_id: string },
) {
  // #region receive
  const topic = Buffer.from(hint.topic, "base64");
  if (!/^[0-9]+$/.test(hint.sequence_id)) {
    throw new Error("Invalid push sequence");
  }
  const sequenceId = BigInt(hint.sequence_id);
  const isGroup = topic[0] === 0 && topic.length === 17;
  const isWelcome = topic[0] === 1 && topic.length === 33;
  if (!isGroup && !isWelcome) throw new Error("Invalid push topic");

  // Fetch and process welcomes before looking up the group.
  await client.conversations.sync();
  if (isWelcome) return;
  const groupId = topic.subarray(1).toString("hex");
  const conversation = await client.conversations.getConversationById(groupId);
  if (!conversation) throw new Error("Conversation is not available yet");
  await conversation.sync();
  const messages = await conversation.messages();
  // #endregion receive
  return { sequenceId, messages };
}
