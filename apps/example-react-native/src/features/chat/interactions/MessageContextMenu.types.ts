import type { ReactElement } from "react";

import type { MessageMenuItem } from "@/core/messageInteractions.ts";
import type { AttachmentAction } from "@/features/attachments/AttachmentActions";

export interface MessageContextMenuProps {
  /** The menu items, in order (`menuItems`). */
  readonly items: readonly MessageMenuItem[];
  readonly attachmentActions?: readonly AttachmentAction[];
  /** An inner attachment press target can request the same message menu. */
  readonly openRequest?: number;
  /** The text the preview shows under the quick reactions. */
  readonly previewText: string;
  /** Shows the quick reactions and "More" (the conversation is allowed). */
  readonly quickReactions: boolean;
  readonly onItem: (item: MessageMenuItem) => void;
  /** A quick reaction (toggles it). */
  readonly onReact: (emoji: string) => void;
  /** "More": the emoji picker. */
  readonly onMore: () => void;
  /** The bubble with its badges: one view, the long-press target. */
  readonly children: ReactElement;
}
