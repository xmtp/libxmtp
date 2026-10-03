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
  /** The SDK content record with its decoded value. */
  readonly content: MessageContent & {
    /** The decoded value when the content record has one. */
    readonly value?: Content;
  };
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
const isGroupAdminAsync = (
  conversation: Group | Dm,
  message: Message,
): Promise<boolean> =>
  isGroup(conversation)
    ? conversation.isAdmin(message.senderInboxId)
    : Promise.resolve(false);
const isGroupSuperAdminAsync = (
  conversation: Group | Dm,
  message: Message,
): Promise<boolean> =>
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
  /** Check whether the client sent the message. */
  fromSelf,
  /** Check whether the content type is known and has decoded custom content. */
  hasContent,
  /** Check whether the conversation is a direct message. */
  isDM,
  /** Check whether the conversation is a group. */
  isGroup,
  /** Await this check before granting group admin access. */
  isGroupAdminAsync,
  /** Await this check before granting group super admin access. */
  isGroupSuperAdminAsync,
  /** Check the codec authority, type name, and major version. */
  usesCodec,
};
/** Short name for the message and conversation filters. */
export const f = filter;
