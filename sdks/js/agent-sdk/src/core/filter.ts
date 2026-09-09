import {
  contentTypesAreEqual,
  type ContentCodec,
} from "@xmtp/content-type-primitives";
import {
  Dm,
  Group,
  type Client,
  type Conversation,
  type DecodedMessage,
} from "@xmtp/node-sdk";

/** A decoded message whose content is known to be present. */
export type DecodedMessageWithContent<ContentTypes = unknown> =
  DecodedMessage<ContentTypes> & {
    /** The decoded content after the presence check. */
    content: ContentTypes;
  };

const fromSelf = <ContentTypes>(
  message: DecodedMessage<ContentTypes>,
  client: Client<ContentTypes>,
) => {
  return message.senderInboxId === client.inboxId;
};

const hasContent = <ContentTypes>(
  message: DecodedMessage<ContentTypes>,
): message is DecodedMessageWithContent<ContentTypes> => {
  return message.content !== undefined && message.content !== null;
};

const isDM = (conversation: Conversation): conversation is Dm => {
  return conversation instanceof Dm;
};

const isGroup = (conversation: Conversation): conversation is Group => {
  return conversation instanceof Group;
};

const isGroupAdmin = (conversation: Conversation, message: DecodedMessage) => {
  if (isGroup(conversation)) {
    return conversation.isAdmin(message.senderInboxId);
  }
  return false;
};

const isGroupSuperAdmin = (
  conversation: Conversation,
  message: DecodedMessage,
) => {
  if (isGroup(conversation)) {
    return conversation.isSuperAdmin(message.senderInboxId);
  }
  return false;
};

const usesCodec = <T extends ContentCodec>(
  message: DecodedMessage,
  codecClass: new () => T,
): message is DecodedMessageWithContent<ReturnType<T["decode"]>> => {
  return contentTypesAreEqual(
    message.contentType,
    new codecClass().contentType,
  );
};

/** Type guards used by Agent middleware to classify messages and conversations. */
export const filter = {
  /** Return true when a message was sent by the supplied client. */
  fromSelf,
  /** Return true when a message contains decoded content. */
  hasContent,
  /** Return true when a conversation is a direct message. */
  isDM,
  /** Return true when a conversation is a group. */
  isGroup,
  /** Return true when the message sender is a group admin. */
  isGroupAdmin,
  /** Return true when the message sender is a group super admin. */
  isGroupSuperAdmin,
  /** Return true when a message uses the supplied codec. */
  usesCodec,
};

/** Short alias for {@link filter}. */
export const f = filter;
