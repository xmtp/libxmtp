import {
  type Conversation,
  type ConversationID,
  type EncodedContent,
  type MessageContent,
  StandardContent,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function reject(
  conversation: Conversation,
  content: MessageContent,
): void {
  const id: ConversationID = "raw string";
  const encoded: EncodedContent = content;
  const group = conversation.inner.group;
  const invalid = new StandardContent.DeleteMessage({ messageID: "raw string" });
  void id;
  void encoded;
  void group;
  void invalid;
}
