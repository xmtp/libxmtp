import type {
  DeleteMessageContent,
  EncodedContent,
  InboxId,
  MessageId,
  ReactionV2Content,
  ReplyContent,
} from "xmtp-sdk-browser";
import type {
  DeleteMessageContent as PureDeleteMessageContent,
  ReactionV2Content as PureReactionV2Content,
  ReplyContent as PureReplyContent,
} from "xmtp-sdk-browser/pure";

// The worker and pure roots expose the same generated record fields.
export function consumeCodecRecords(
  reference: MessageId,
  inboxId: InboxId,
  content: EncodedContent,
) {
  const reaction: ReactionV2Content = {
    reference,
    reaction: { content: "👍", action: "added", schema: "unicode" },
  };
  const reply: ReplyContent = { reference, content };
  const deletion: DeleteMessageContent = { messageId: reference };
  const reactionWithInbox: PureReactionV2Content = {
    ...reaction,
    referenceInboxId: inboxId,
  };
  const replyWithInbox: PureReplyContent = { ...reply, referenceInboxId: inboxId };
  const pureDeletion: PureDeleteMessageContent = deletion;
  return [reaction, reply, deletion, reactionWithInbox, replyWithInbox, pureDeletion];
}
