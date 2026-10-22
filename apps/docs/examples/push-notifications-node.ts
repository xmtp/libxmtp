import { randomBytes } from "node:crypto";

import {
  parsePushHint,
  type Client,
  type Conversation,
  type NotificationChannel,
} from "@xmtp/node-sdk";

export async function configureNotifications(
  client: Client,
  conversation: Conversation,
  token: string,
) {
  // #region configure
  await client.enableNotifications({ channel: { kind: "fcm", token } });
  await conversation.setNotifications("disabled");
  const conversationState = await conversation.state();
  const enabled =
    "common" in conversationState
      ? conversationState.common.notificationsEnabled
      : conversationState.notificationsEnabled;
  await conversation.setNotifications("default");
  const state = client.notificationState();
  if (state.kind === "failed") {
    console.error(state.error);
  }
  await client.disableNotifications();
  // #endregion configure
  return enabled;
}

export function webhookChannel(url: string) {
  // #region webhook
  const signingKey = randomBytes(32);
  const channel: NotificationChannel = { kind: "http", url, signingKey };
  // #endregion webhook
  return channel;
}

export async function receivePushHint(
  client: Client,
  hint: { topic: string; sequence_id: string },
) {
  // #region receive
  // Throws InvalidArgument for a malformed topic or sequence id.
  const { topic, sequenceId } = parsePushHint(hint.topic, hint.sequence_id);
  // Push subscriptions carry only group and welcome topics.
  if (topic.kind !== "groupMessages" && topic.kind !== "welcomeMessages") {
    throw new Error("Unexpected push topic kind");
  }

  // Fetch and process welcomes before looking up the group.
  await client.conversations.sync();
  if (topic.kind === "welcomeMessages") return;
  const conversation = await client.conversations.getById(topic.conversationId);
  if (!conversation) throw new Error("Conversation is not available yet");
  await conversation.sync();
  const messages = await conversation.messages();
  // #endregion receive
  return { sequenceId, messages };
}
