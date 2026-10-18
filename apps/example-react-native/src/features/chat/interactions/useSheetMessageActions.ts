import { useEffect, useState } from "react";

import type { MessageActions } from "@/core/messageActions.ts";
import { createMessageActions } from "@/features/chat/chatRuntime";
import type { ActiveSession } from "@/session";

export interface SheetMessageActions {
  readonly actions: MessageActions;
  /** False once the sheet's content unmounted. */
  readonly isMounted: () => boolean;
}

/**
 * The message actions of a sheet over the chat (Reactions, emoji picker):
 * loads `messageId` and admits results only while the sheet is mounted.
 */
export function useSheetMessageActions(
  session: ActiveSession,
  conversationId: string,
  messageId: string,
): SheetMessageActions {
  const [mounted] = useState(() => ({ current: true }));
  const [value] = useState<SheetMessageActions>(() => ({
    actions: createMessageActions(
      session,
      conversationId,
      () => mounted.current,
    ),
    isMounted: () => mounted.current,
  }));
  const { actions } = value;
  useEffect(() => {
    mounted.current = true;
    actions.load(messageId);
    return () => {
      mounted.current = false;
    };
  }, [actions, messageId, mounted]);
  return value;
}
