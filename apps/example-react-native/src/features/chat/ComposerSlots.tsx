// Composer slots the attachments lane fills: the "+" attach menu at the start
// of the input row, and the staged attachment cards above the error line. Both
// receive the chat from `useChatContext()`.

export interface ComposerSlotProps {
  readonly conversationId: string;
  /** The conversation's consent is allowed. */
  readonly allowed: boolean;
}

/**
 * The "+" attach menu (glass, title3 plus, 44 pt, "Add attachment"), shown
 * when the server offers attachments and the conversation is allowed.
 * Renders nothing until the attachments lane adds it.
 */
export function ComposerAttachMenu(_props: ComposerSlotProps) {
  return null;
}

/**
 * Staged attachment cards of this conversation (Send, Retry, Discard,
 * "Review send"), plus attachment errors. Renders nothing until the
 * attachments lane adds it.
 */
export function StagedAttachments(_props: ComposerSlotProps) {
  return null;
}
