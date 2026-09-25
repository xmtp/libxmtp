import {
  Conversation_Tags,
  MessageContent_Tags,
  type Conversation,
  type ConversationID,
  type EncodedContent,
  type Message,
  type MessageID,
  type InboxID,
  type MessageContent,
  type StandardContent,
  StandardContent_Tags,
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

export function consumeStandardIDs(content: StandardContent): MessageID | undefined {
  if (content.tag === StandardContent_Tags.Reaction) {
    const reference: MessageID = content.inner.reference;
    const inbox: InboxID | undefined = content.inner.referenceInboxID;
    void inbox;
    return reference;
  }
  if (content.tag === StandardContent_Tags.Reply) {
    const reference: MessageID = content.inner.reference;
    return reference;
  }
  if (content.tag === StandardContent_Tags.DeleteMessage) {
    const id: MessageID = content.inner.messageID;
    return id;
  }
  return undefined;
}

export async function consumeMessageConversation(
  message: Message,
): Promise<Conversation | undefined> {
  return message.conversation();
}
