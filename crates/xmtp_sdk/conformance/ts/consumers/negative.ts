import {
  type Conversation,
  type ConversationID,
  type EncodedContent,
  type MessageContent,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function reject(
  conversation: Conversation,
  content: MessageContent,
): void {
  const id: ConversationID = "raw string";
  const encoded: EncodedContent = content;
  const group = conversation.inner.group;
  void id;
  void encoded;
  void group;
}
