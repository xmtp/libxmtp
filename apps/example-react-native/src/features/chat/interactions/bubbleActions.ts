import { router } from "expo-router";

import type { MessageMenuItem } from "@/core/messageInteractions.ts";
import type { ChatRuntime } from "@/features/chat/chatRuntime";
import { copyText } from "@/platform/clipboard";

/** Opens the emoji picker sheet for a message. */
export function openEmojiPicker(
  conversationId: string,
  messageId: string,
): void {
  router.push({
    pathname: "/emoji-picker/[conversationId]/[messageId]",
    params: { conversationId, messageId },
  });
}

/**
 * Runs a context-menu or VoiceOver item on a row of the open chat. Delete only
 * asks for the confirmation; the chat's interactions layer shows the "Delete
 * message?" alert.
 */
export function runMenuItem(
  runtime: ChatRuntime,
  messageId: string,
  item: MessageMenuItem,
): void {
  switch (item) {
    case "copy": {
      const text = runtime.copyText(messageId);
      if (text === undefined) return;
      copyText(text).catch((error: unknown) => {
        console.warn("Cannot copy the message", error);
      });
      return;
    }
    case "reply":
      runtime.startReply(messageId);
      return;
    case "react":
      openEmojiPicker(runtime.conversationId, messageId);
      return;
    case "delete":
      runtime.requestDelete(messageId);
      return;
    case "retry":
      runtime.retry(messageId);
      return;
    case "edit":
      runtime.edit(messageId);
      return;
  }
}
