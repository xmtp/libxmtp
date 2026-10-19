import { type ComponentRef, type RefObject, createContext, use } from "react";
import type { View } from "react-native";

import type { ChatRuntime } from "@/features/chat/chatRuntime";

export interface ChatContextValue {
  readonly runtime: ChatRuntime;
  readonly conversationId: string;
  /** The composer block (the send flight starts at its center). */
  readonly composerRef: RefObject<ComponentRef<typeof View> | null>;
}

export const ChatContext = createContext<ChatContextValue | undefined>(
  undefined,
);

/** The open chat's runtime and refs, for components inside the chat screen. */
export function useChatContext(): ChatContextValue {
  const value = use(ChatContext);
  if (value === undefined) {
    throw new Error("useChatContext needs the chat screen.");
  }
  return value;
}
