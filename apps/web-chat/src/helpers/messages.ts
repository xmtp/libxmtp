import type { Message, MessageBody, MessageContent } from "@xmtp/browser-sdk";

import { jsonStringify } from "@/helpers/strings";

type WithContent<K extends MessageContent["kind"]> = Message & {
  content: Extract<MessageContent, { kind: K }>;
};
export const isReaction = (
  message: Message,
): message is WithContent<"reaction"> => message.content.kind === "reaction";
export const isReply = (message: Message): message is WithContent<"reply"> =>
  message.content.kind === "reply";
export const isTextReply = (
  message: Message,
): message is WithContent<"reply"> & {
  content: { body: Extract<MessageBody, { kind: "text" }> };
} => isReply(message) && message.content.body.kind === "text";
export const isText = (message: Message): message is WithContent<"text"> =>
  message.content.kind === "text";
export const isRemoteAttachment = (
  message: Message,
): message is WithContent<"remoteAttachment"> =>
  message.content.kind === "remoteAttachment";
export const stringify = (message: Message): string => {
  const content = message.content;
  if (content.kind === "reaction") return content.reaction.content;
  if (content.kind === "reply" && content.body.kind === "text")
    return content.body.value;
  if (content.kind === "text" || content.kind === "markdown")
    return content.value;
  return message.fallback ?? jsonStringify(content);
};
export const isActionable = (message: Message) =>
  isText(message) ||
  isReaction(message) ||
  isTextReply(message) ||
  isRemoteAttachment(message);
