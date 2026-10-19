// Composer slots the attachments lane fills: the "+" attach menu at the start
// of the input row, and the staged attachment cards above the error line. Both
// receive the chat from `useChatContext()`.

import {
  AttachMenu,
  StagedAttachments as StagedAttachmentList,
} from "@/features/attachments/ComposerAttachments";

export interface ComposerSlotProps {
  readonly conversationId: string;
  /** The conversation's consent is allowed. */
  readonly allowed: boolean;
}

/**
 * The "+" attach menu (glass, title3 plus, 44 pt, "Add attachment"), shown
 * when the server offers attachments and the conversation is allowed.
 */
export function ComposerAttachMenu(props: ComposerSlotProps) {
  return <AttachMenu {...props} />;
}

/**
 * Staged attachment cards of this conversation (Send, Retry, Discard,
 * "Review send"), plus attachment errors.
 */
export function StagedAttachments(props: ComposerSlotProps) {
  return <StagedAttachmentList {...props} />;
}
