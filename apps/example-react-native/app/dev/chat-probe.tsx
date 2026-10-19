// Debug-only actions on an open chat, for checks that need states the shared
// backend cannot produce or UI that another lane adds (context menu actions).
// `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/chat-probe?action=<a>&c=<conversation>&m=<message>&n=<new value>"`
// Actions: `failNextSend` (the next send fails after it may have queued: an
// unknown outcome), `failPublish` and `allowPublish` (publishing fails until
// allowed again), `reply`, `retry`, `edit`, `delete`, `react` (`&e=<emoji>`).
// The probe runs the action on the mounted chat, then goes back to it.

import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useRef } from "react";

import { mountedChat } from "@/features/chat/chatRuntime";
import { chatDebug } from "@/features/chat/sdkChatQueries";
import { useActiveSession } from "@/session";

export default function ChatProbe() {
  const session = useActiveSession();
  const params = useLocalSearchParams<{
    action?: string;
    c?: string;
    m?: string;
    e?: string;
    n?: string;
  }>();
  const ran = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!__DEV__ || session === undefined || params.action === undefined) {
      return;
    }
    const id = `${params.action}|${params.m ?? ""}|${params.n ?? ""}`;
    if (ran.current === id) return;
    ran.current = id;
    const chat =
      params.c === undefined ? undefined : mountedChat(params.c, session);
    // Without `m`, the newest own row that is not published.
    const message =
      params.m ??
      chat?.timeline
        .getState()
        .messages.findLast(
          (row) => row.sender === chat.own && row.delivery !== "published",
        )?.id ??
      "";
    console.log(`[chat-probe] message ${message}`);
    switch (params.action) {
      case "failNextSend":
        chatDebug.failNextQueue = true;
        break;
      case "failPublish":
        chatDebug.failPublish = true;
        break;
      case "allowPublish":
        chatDebug.failPublish = false;
        break;
      case "reply":
        chat?.startReply(message);
        break;
      case "retry":
        chat?.retry(message);
        break;
      case "edit":
        chat?.edit(message);
        break;
      case "delete":
        chat?.deleteMessage(message);
        break;
      case "react":
        chat?.react(message, params.e ?? "👍");
        break;
      default:
        console.warn(`[chat-probe] unknown action ${params.action}`);
    }
    console.log(
      `[chat-probe] ${params.action} ${chat ? "on chat" : "no chat"}`,
    );
    if (router.canGoBack()) router.back();
  }, [params.action, params.c, params.e, params.m, params.n, session]);

  // The root layout registers this route for Debug builds only, with its
  // title. The probe goes back as soon as its action ran.
  return null;
}
