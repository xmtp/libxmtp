// A consumer of the Node and browser package roots. It uses public values
// only: `kind` unions, getters, `Uint8Array`, and optional options.
import type * as Browser from "../../../../../target/sdk-generated/typescript-wasm/index.ts";
import type {
  Actions,
  Attachment,
  Conversation,
  ConversationId,
  Conversations,
  EncodedContent,
  Group,
  InboxId,
  Intent,
  Message,
  MessageContent,
  MessageId,
  MultiRemoteAttachment,
  Reaction,
  RemoteAttachment,
  StandardContent,
  TransactionReference,
  WalletSendCalls,
} from "../../../../../target/sdk-generated/typescript-napi/index.ts";
import type * as Node from "../../../../../target/sdk-generated/typescript-napi/index.ts";

export function consume(
  id: ConversationId,
  conversation: Conversation,
  content: MessageContent,
): [ConversationId, EncodedContent | undefined] {
  if (conversation.kind === "group") {
    const groupId: ConversationId = conversation.id;
    id = groupId;
  } else {
    const dmId: ConversationId = conversation.id;
    id = dmId;
  }
  if (content.kind === "custom") {
    const bytes: Uint8Array = content.encoded.content;
    void bytes;
    return [id, content.encoded];
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
  if (content.kind === "reaction") {
    const reference: MessageId = content.reference;
    const inbox: InboxId | undefined = content.referenceInboxId;
    void inbox;
    return reference;
  }
  if (content.kind === "reply") {
    const reference: MessageId = content.reference;
    return reference;
  }
  if (content.kind === "deleteMessage") {
    const id: MessageId = content.messageId;
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
  if (message.content.kind === "custom") {
    values.push(message.content.value);
  }
  if (message.replyContent?.kind === "custom") {
    values.push(message.replyContent.value);
  }
  return values;
}

// verifies: PROC-050, DMS-017
export async function consumeReaderSurface(
  NodeSdk: typeof Node,
  BrowserSdk: typeof Browser,
  node: Node.Client,
  nodeGroup: Node.Group,
  nodeDm: Node.Dm,
  browser: Browser.Client,
  browserGroup: Browser.Group,
  browserDm: Browser.Dm,
): Promise<void> {
  const signal = new AbortController().signal;
  await node.conversations.messageReader();
  await node.conversations.messageReader({
    consentStates: [],
    conversationKind: undefined,
  });
  await browser.conversations.messageReader();
  await browser.conversations.messageReader({ consentStates: [] });
  for (const named of [nodeGroup, nodeDm, browserGroup, browserDm]) {
    await named.messageReader();
    await named.messageReader({ from: undefined });
  }
  const received: Array<string | null> = [
    nodeGroup.creatorInboxId,
    nodeGroup.addedByInboxId,
    nodeDm.creatorInboxId,
    nodeDm.addedByInboxId,
    browserGroup.creatorInboxId,
    browserGroup.addedByInboxId,
    browserDm.creatorInboxId,
    browserDm.addedByInboxId,
  ];
  void received;
  const nodePeer: string | null = await nodeDm.peerInboxId();
  const browserPeer: string | null = await browserDm.peerInboxId();
  void [nodePeer, browserPeer];
  NodeSdk.MessageStream.open(node);
  NodeSdk.MessageStream.open(node, { consentStates: [] }, { signal });
  NodeSdk.MessageStream.openGroup(node, nodeGroup);
  NodeSdk.MessageStream.openDm(node, nodeDm, { from: undefined }, { signal });
  BrowserSdk.MessageStream.open(browser);
  BrowserSdk.MessageStream.open(browser, { consentStates: [] }, { signal });
  BrowserSdk.MessageStream.openGroup(browser, browserGroup);
  BrowserSdk.MessageStream.openDm(
    browser,
    browserDm,
    { from: undefined },
    { signal },
  );
}
