import {
  Dm,
  Group,
  type AnyContentCodec,
  type Client,
  type Message,
  type MessageContent,
} from "@xmtp/node-sdk";

/** A message with decoded content. */
export type DecodedMessageWithContent<Content = unknown> = Message & {
  readonly content: MessageContent & { readonly value?: Content };
};

const fromSelf = (message: Message, client: Client) =>
  message.senderInboxId === client.inboxId;
const hasContent = (message: Message): message is DecodedMessageWithContent =>
  message.content.kind !== "unknown" &&
  (message.content.kind !== "custom" || "value" in message.content);
const isDM = (conversation: Group | Dm): conversation is Dm =>
  conversation instanceof Dm;
const isGroup = (conversation: Group | Dm): conversation is Group =>
  conversation instanceof Group;
const isGroupAdmin = (conversation: Group | Dm, message: Message) =>
  isGroup(conversation)
    ? conversation.isAdmin(message.senderInboxId)
    : Promise.resolve(false);
const isGroupSuperAdmin = (conversation: Group | Dm, message: Message) =>
  isGroup(conversation)
    ? conversation.isSuperAdmin(message.senderInboxId)
    : Promise.resolve(false);
const usesCodec = <T extends AnyContentCodec>(
  message: Message,
  codecClass: new () => T,
): message is DecodedMessageWithContent<ReturnType<T["decode"]>> => {
  const type = new codecClass().type;
  const actual = message.contentType;
  return (
    actual !== undefined &&
    actual.authorityId === type.authorityId &&
    actual.typeId === type.typeId &&
    actual.versionMajor === type.versionMajor
  );
};

/** Message and conversation tests for agent middleware. */
export const filter = {
  fromSelf,
  hasContent,
  isDM,
  isGroup,
  isGroupAdmin,
  isGroupSuperAdmin,
  usesCodec,
};
export const f = filter;
