import type { ReactNode } from "react";

import type { ChatMessage } from "@/core/chatMessages.ts";
import { MessageInteractions } from "@/features/chat/interactions/MessageInteractions";

export interface MessageBubbleWrapperProps {
  readonly row: ChatMessage;
  /** The own inbox sent the row. */
  readonly mine: boolean;
  /** The conversation's consent is allowed (Reply, React, Delete, Retry, Edit). */
  readonly allowed: boolean;
  /** The bubble with its reaction badges. */
  readonly children: ReactNode;
}

/**
 * The interaction layer around one bubble (`ChatMessages.swift`
 * `messageBubble` modifiers): long-press context menu with the quick-reaction
 * preview, swipe to reply, and hiding the bubble while its send flight runs.
 * See `features/chat/interactions`.
 */
export function MessageBubbleWrapper(props: MessageBubbleWrapperProps) {
  return <MessageInteractions {...props} />;
}
