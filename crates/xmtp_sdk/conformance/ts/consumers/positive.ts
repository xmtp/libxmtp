import {
  Conversation_Tags,
  MessageContent_Tags,
  type Conversation,
  type ConversationID,
  type EncodedContent,
  type MessageContent,
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
