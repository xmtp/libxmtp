import {
  Conversation_Tags,
  MessageBody_Tags,
  MessageContent_Tags,
  type Actions,
  type Attachment,
  type Conversation,
  type ConversationId,
  type Conversations,
  type EncodedContent,
  type Group,
  type InboxId,
  type Intent,
  type Message,
  type MessageContent,
  type MessageId,
  type MultiRemoteAttachment,
  type Reaction,
  type RemoteAttachment,
  type StandardContent,
  StandardContent_Tags,
  type TransactionReference,
  type WalletSendCalls,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function consume(
  id: ConversationId,
  conversation: Conversation,
  content: MessageContent,
): [ConversationId, EncodedContent | undefined] {
  if (conversation.tag === Conversation_Tags.Group) {
    const groupId: ConversationId = conversation.inner.group.id();
    id = groupId;
  } else {
    const dmId: ConversationId = conversation.inner.dm.id();
    id = dmId;
  }
  if (content.tag === MessageContent_Tags.Custom) {
    return [id, content.inner.encoded];
  }
  return [id, undefined];
}

export async function consumeOmittedSendOptions(
  group: Group,
  conversations: Conversations,
  id: MessageId,
  reaction: Reaction,
  encoded: EncodedContent,
): Promise<void> {
  await group.send(encoded);
  await group.prepareMessage(encoded);
  await conversations.reactToMessage(id, reaction);
  await conversations.replyToMessage(id, encoded);
}

export async function consumeOmittedTypedSendOptions(
  group: Group,
  id: MessageId,
  reaction: Reaction,
  encoded: EncodedContent,
  attachment: Attachment,
  remote: RemoteAttachment,
  multiRemote: MultiRemoteAttachment,
  transaction: TransactionReference,
  walletCalls: WalletSendCalls,
  actions: Actions,
  intent: Intent,
): Promise<void> {
  await group.sendText("text");
  await group.sendMarkdown("markdown");
  await group.sendReaction(id, undefined, reaction);
  await group.sendReply(id, undefined, encoded);
  await group.sendReadReceipt();
  await group.sendAttachment(attachment);
  await group.sendRemoteAttachment(remote);
  await group.sendMultiRemoteAttachment(multiRemote);
  await group.sendTransactionReference(transaction);
  await group.sendWalletSendCalls(walletCalls);
  await group.sendActions(actions);
  await group.sendIntent(intent);
}

export function consumeStandardIds(
  content: StandardContent,
): MessageId | undefined {
  if (content.tag === StandardContent_Tags.Reaction) {
    const reference: MessageId = content.inner.reference;
    const inbox: InboxId | undefined = content.inner.referenceInboxId;
    void inbox;
    return reference;
  }
  if (content.tag === StandardContent_Tags.Reply) {
    const reference: MessageId = content.inner.reference;
    return reference;
  }
  if (content.tag === StandardContent_Tags.DeleteMessage) {
    const id: MessageId = content.inner.messageId;
    return id;
  }
  return undefined;
}

export async function consumeMessageConversation(
  message: Message,
): Promise<Conversation | undefined> {
  return message.conversation();
}

export function consumeLiftedCustomValues(message: Message): unknown[] {
  const values: unknown[] = [];
  if (message.content.tag === MessageContent_Tags.Custom) {
    values.push(message.content.inner.value);
  }
  if (message.replyContent?.tag === MessageBody_Tags.Custom) {
    values.push(message.replyContent.inner.value);
  }
  return values;
}
