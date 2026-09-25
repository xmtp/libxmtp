import {
  Conversation_Tags,
  MessageContent_Tags,
  type Conversation,
  type ConversationID,
  type Conversations,
  type EncodedContent,
  type Group,
  type Message,
  type MessageContent,
  type MessageID,
  type Reaction,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function consume(
  id: ConversationID,
  conversation: Conversation,
  content: MessageContent,
): [ConversationID, EncodedContent | undefined] {
  if (conversation.tag === Conversation_Tags.Group) {
    const groupID: ConversationID = conversation.inner.group.id();
    id = groupID;
  } else {
    const dmID: ConversationID = conversation.inner.dm.id();
    id = dmID;
  }
  if (content.tag === MessageContent_Tags.Custom) {
    return [id, content.inner.encoded];
  }
  return [id, undefined];
}

export async function consumeOmittedSendOptions(
  group: Group,
  conversations: Conversations,
  id: MessageID,
  reaction: Reaction,
  encoded: EncodedContent,
): Promise<void> {
  await group.send(encoded);
  await group.prepareMessage(encoded);
  await conversations.reactToMessage(id, reaction);
  await conversations.replyToMessage(id, encoded);
}

export async function consumeMessageConversation(
  message: Message,
): Promise<Conversation | undefined> {
  return message.conversation();
}
