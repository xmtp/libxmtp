// Each statement must fail to compile against the Node package root: the root
// shows no binding shape, factory, transport member, or wrong-typed value.
import {
  Group,
  MainSession,
  StandardContent_Tags,
  type Client,
  type Conversation,
  type ConversationId,
  type EncodedContent,
  type MessageContent,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function reject(
  client: Client,
  group: Group,
  conversation: Conversation,
  content: MessageContent,
): void {
  const id: ConversationId = 42;
  const encoded: EncodedContent = content;
  const tag = conversation.tag;
  const inner = content.inner;
  const created = new Group();
  const factory = Group.new();
  const conversations = client.conversations();
  const handle = group.handle;
  void [id, encoded, tag, inner, created, factory, conversations, handle];
  void [MainSession, StandardContent_Tags];
}
