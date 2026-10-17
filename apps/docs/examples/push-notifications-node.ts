import { randomBytes } from "node:crypto";

import type { Client, Conversation, NotificationChannel } from "@xmtp/node-sdk";

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
  const topic = Buffer.from(hint.topic, "base64");
  if (topic.toString("base64") !== hint.topic) {
    throw new Error("Invalid push topic encoding");
  }
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

  // Fetch and process welcomes before looking up the group.
  await client.conversations.sync();
  if (isWelcome) return;
  const groupId = topic.subarray(1).toString("hex");
  const conversation = await client.conversations.getById(groupId);
  if (!conversation) throw new Error("Conversation is not available yet");
  await conversation.sync();
  const messages = await conversation.messages();
  // #endregion receive
  return { sequenceId, messages };
}
