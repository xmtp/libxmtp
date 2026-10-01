import type {
  DeleteMessageContent,
  EncodedContent,
  InboxId,
  MessageId,
  ReactionV2Content,
  ReplyContent,
} from "xmtp-sdk";

// The public records come from Rust metadata, including absent inbox defaults.
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
  const reactionWithInbox: ReactionV2Content = {
    ...reaction,
    referenceInboxId: inboxId,
  };
  const replyWithInbox: ReplyContent = { ...reply, referenceInboxId: inboxId };
  return [reaction, reply, deletion, reactionWithInbox, replyWithInbox];
}
