import {
  parsePushHint,
  type Client,
  type Conversation,
  type NotificationChannel,
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
) {
  // #region receive
  // Throws InvalidArgument for a malformed topic or sequence id.
  const { topic, sequenceId } = parsePushHint(hint.topic, hint.sequence_id);
  // Push subscriptions carry only group and welcome topics.
  if (topic.kind === "welcomeMessages") {
    await client.conversations.sync();
    return;
  }
  if (topic.kind !== "groupMessages") {
    throw new Error("Unexpected push topic kind");
  }
  let conversation = await client.conversations.getById(topic.conversationId);
  if (!conversation) {
    await client.conversations.sync();
    conversation = await client.conversations.getById(topic.conversationId);
  }
  if (!conversation) throw new Error("Conversation is not available yet");
  await conversation.sync();
  const messages = await conversation.messages();
  // #endregion receive
  return { sequenceId, messages };
}
