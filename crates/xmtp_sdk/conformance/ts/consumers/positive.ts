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

// verifies: PROC-050, DMS-017
export async function consumeReaderSurface(
  node: import("../../../../../target/sdk-generated/typescript-napi/index.ts").Client,
  nodeGroup: import("../../../../../target/sdk-generated/typescript-napi/index.ts").Group,
  nodeDm: import("../../../../../target/sdk-generated/typescript-napi/index.ts").Dm,
  browser: import("../../../../../target/sdk-generated/typescript-wasm/index.ts").Client,
  browserGroup: import("../../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts").Group,
  browserDm: import("../../../../../target/sdk-generated/typescript-wasm/proxy.gen.ts").Dm,
): Promise<void> {
  const N =
    await import("../../../../../target/sdk-generated/typescript-napi/index.ts");
  const B =
    await import("../../../../../target/sdk-generated/typescript-wasm/index.ts");
  const signal = new AbortController().signal;
  for (const conversations of [node.conversations(), browser.conversations()]) {
    await conversations.messageReader();
    await conversations.messageReader(
      { consentStates: [], from: undefined, conversationKind: undefined },
      { signal },
    );
  }
  for (const named of [nodeGroup, nodeDm, browserGroup, browserDm]) {
    await named.messageReader();
    await named.messageReader({ from: undefined }, { signal });
  }
  const received: Array<string | null> = [
    nodeGroup.creatorInboxId(),
    nodeGroup.addedByInboxId(),
    nodeDm.creatorInboxId(),
    nodeDm.addedByInboxId(),
    browserGroup.creatorInboxId(),
    browserGroup.addedByInboxId(),
    browserDm.creatorInboxId(),
    browserDm.addedByInboxId(),
  ];
  void received;
  const nodePeer: string | null = await nodeDm.peerInboxId();
  const browserPeer: string | null = await browserDm.peerInboxId();
  const browserLike: import("../../../../../target/sdk-generated/typescript-wasm/xmtp_sdk.ts").DmLike =
    browserDm;
  void [nodePeer, browserPeer, browserLike];
  N.MessageStream.open(node);
  N.MessageStream.open(
    node,
    { consentStates: [], from: undefined, conversationKind: undefined },
    { signal },
  );
  N.MessageStream.openGroup(node, nodeGroup);
  N.MessageStream.openDm(node, nodeDm, { from: undefined }, { signal });
  B.MessageStream.open(browser);
  B.MessageStream.open(
    browser,
    { consentStates: [], from: undefined, conversationKind: undefined },
    { signal },
  );
  B.MessageStream.openGroup(browser, browserGroup);
  B.MessageStream.openDm(browser, browserDm, { from: undefined }, { signal });
}
