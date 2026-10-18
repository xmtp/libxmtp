/** Chat strings (`Features/Chat/Chat.xcstrings` and the chat views). */
export const chatCopy = {
  fallbackTitle: "Conversation",
  /** "· 1 member" / "· N members". */
  memberCount: (count: number) =>
    count === 1 ? "· 1 member" : `· ${String(count)} members`,
  olderMessages: "Older messages",
  olderPending: "Older pending messages",
  newestPending: "Newest pending messages",
  jumpToLatest: "Jump to latest",
  retry: "Retry",
  sent: "Sent",
  sending: "Sending",
  notSent: "Not sent · Tap to retry or edit",
  // Composer.
  messagePlaceholder: "Message",
  sendMessage: "Send message",
  replyTo: (text: string | undefined) => `Reply to ${text ?? "message"}`,
  cancelReply: "Cancel reply",
  reviewSend: "Review send",
  // Request bar.
  messageRequest: "Message request",
  requestDetail:
    "Accept to reply. Request media stays closed until you choose to download it.",
  senderDetails: "Sender details",
  accept: "Accept",
  deleteRequest: "Delete request",
  block: "Block",
  confirmRequestAction: "Confirm request action",
  confirm: "Confirm",
  // Settings › Send needs review (`SendReview.xcstrings`; the rest is `SendReviewCopy`).
  sendReviewTitle: "Send needs review",
  // Attachment placeholder (the attachments lane replaces the card).
  attachmentFallback: "File",
} as const;
