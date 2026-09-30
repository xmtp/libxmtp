import type {
  Client,
  Conversation,
  Dm,
  Group,
  Message,
  MessageId,
  PublicIdentity,
  Reaction,
} from "xmtp-sdk";

// Received identity is absent as null, never undefined.
export async function consumeIdentity(
  client: Client,
  identity: PublicIdentity,
  group: Group,
  dm: Dm,
): Promise<void> {
  const conversations = client.conversations;
  // Each member argument is a union of inbox IDs and account identities.
  const inbox = client.inboxId;
  await conversations.createGroup([inbox]);
  await conversations.createGroup([identity]);
  await conversations.createGroup([]);
  await conversations.createDm(inbox);
  await conversations.createDm(identity);
  await group.addMembers([inbox]);
  await group.addMembers([identity]);
  await group.removeMembers([inbox]);
  await group.removeMembers([identity]);
  const peer: string | null = await dm.peerInboxId();
  const groupCreator: string | null = group.creatorInboxId;
  const groupAdder: string | null = group.addedByInboxId;
  const dmCreator: string | null = dm.creatorInboxId;
  const dmAdder: string | null = dm.addedByInboxId;
  const isCreator: boolean = group.isCreator;
  void [peer, groupCreator, groupAdder, dmCreator, dmAdder, isCreator];
}

export async function consumeMessageActions(
  message: Message,
  reaction: Reaction,
): Promise<void> {
  const owner: Client = message.client();
  const refreshed: Message | undefined = await message.refresh();
  const reacted: MessageId = await message.react(reaction);
  const replied: MessageId = await message.reply("reply");
  const parent: Message | undefined = await message.parent();
  const conversation: Conversation | undefined = await message.conversation();
  const cursor: string | null = message.deliveryCursor;
  const deleted: MessageId = await message.delete();
  await message.deleteLocally();
  void [owner, refreshed, reacted, replied, parent, conversation, cursor];
  void deleted;
}
