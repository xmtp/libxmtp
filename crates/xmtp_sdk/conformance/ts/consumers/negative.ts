import {
  type Conversation,
  type ConversationId,
  type EncodedContent,
  type MessageContent,
  type MessageId,
  StandardContent,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function reject(
  conversation: Conversation,
  content: MessageContent,
): void {
  const id: ConversationId = 42;
  const encoded: EncodedContent = content;
  const group = conversation.inner.group;
  const invalid = new StandardContent.DeleteMessage({ messageId: 42 });
  const removedFactory = MessageId.fromString("bad");
  void id;
  void encoded;
  void group;
  void invalid;
  void removedFactory;
}
