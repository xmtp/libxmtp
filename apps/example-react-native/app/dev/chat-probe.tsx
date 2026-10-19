// Debug-only actions on an open chat, for checks that need states the shared
// backend cannot produce or UI that another lane adds (context menu actions).
// `xcrun simctl openurl <udid> "xmtp-messenger-rn:///dev/chat-probe?action=<a>&c=<conversation>&m=<message>&n=<new value>"`
// Actions: `failNextSend` (the next send fails after it may have queued: an
// unknown outcome), `failPublish` and `allowPublish` (publishing fails until
// allowed again), `reply`, `retry`, `edit`, `delete`, `react` (`&e=<emoji>`).
// The probe runs the action on the mounted chat, then goes back to it. Any app
// can open the URL scheme, so a link that changes data asks in the app first
// (`src/core/probeLinks.ts`); other chat probe links are ignored until that
// alert closes.

import { useLocalSearchParams, useNavigation, useRoute } from "expo-router";
import { StackActions } from "expo-router/react-navigation";
import { useCallback, useEffect, useRef, useState } from "react";

import { MessageMenuCopy } from "@/core/messageInteractions.ts";
import {
  type ChatProbeLinkConfirmation,
  chatProbeLinkConfirmation,
} from "@/core/probeLinks.ts";
import { mountedChat } from "@/features/chat/chatRuntime";
import { chatDebug } from "@/features/chat/sdkChatQueries";
import { probeLinkCopy } from "@/features/dev/copy";
import { useActiveSession } from "@/session";
import { AlertDialog, type AlertDialogRequest } from "@/ui/AlertDialog";

/** The in-app confirmation a link needs (the chat's alert text for Delete). */
function linkConfirmation(
  confirmation: ChatProbeLinkConfirmation,
  emoji: string,
  run: () => void,
): AlertDialogRequest {
  const ask = (
    title: string,
    message: string,
    action: string,
  ): AlertDialogRequest => ({
    title,
    message,
    buttons: [
      { text: action, style: "destructive", onPress: run },
      { text: probeLinkCopy.cancel, style: "cancel" },
    ],
  });
  const title = probeLinkCopy.title(confirmation);
  switch (confirmation) {
    case "delete":
      return ask(
        MessageMenuCopy.deleteTitle,
        MessageMenuCopy.deleteMessage,
        MessageMenuCopy.deleteConfirm,
      );
    case "retry":
      return ask(title, probeLinkCopy.retryMessage, probeLinkCopy.run);
    case "react":
      return ask(title, probeLinkCopy.react(emoji), probeLinkCopy.run);
    case "reply":
      return ask(title, probeLinkCopy.reply, probeLinkCopy.run);
    case "edit":
      return ask(title, probeLinkCopy.edit, probeLinkCopy.run);
  }
}

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
  const [alert, setAlert] = useState<AlertDialogRequest>();
  const navigation = useNavigation();
  const routeKey = useRoute().key;
  // Pops this probe's own route, also when a link to another screen pushed
  // it over the probe while the alert waited.
  const leave = useCallback(() => {
    const stack = navigation.getState();
    if (stack === undefined) return;
    if (stack.routes.findIndex((route) => route.key === routeKey) <= 0) return;
    navigation.dispatch({
      ...StackActions.pop(),
      source: routeKey,
      target: stack.key,
    });
  }, [navigation, routeKey]);

  useEffect(() => {
    if (!__DEV__ || session === undefined || params.action === undefined) {
      return;
    }
    const id = `${params.action}|${params.m ?? ""}|${params.n ?? ""}`;
    if (ran.current === id) return;
    ran.current = id;
    // One link at a time: the open alert's button would run a newer link's
    // action under the text of the first.
    if (alert !== undefined) {
      console.log(
        `[chat-probe] ${params.action} from a link: ignored while another link waits`,
      );
      return;
    }
    const emoji = params.e ?? "👍";
    const run = () => {
      // Read when the action runs, so a confirmed link acts on the chat as
      // it is then.
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
          chat?.react(message, emoji);
          break;
        default:
          console.warn(`[chat-probe] unknown action ${params.action}`);
      }
      console.log(
        `[chat-probe] ${params.action} ${chat ? "on chat" : "no chat"}`,
      );
    };
    const confirmation = chatProbeLinkConfirmation(params.action);
    if (confirmation === undefined) {
      run();
      leave();
      return;
    }
    console.log(
      `[chat-probe] ${params.action} from a link: waiting for confirmation`,
    );
    setAlert(linkConfirmation(confirmation, emoji, run));
  }, [
    alert,
    leave,
    params.action,
    params.c,
    params.e,
    params.m,
    params.n,
    session,
  ]);

  // The root layout registers this route for Debug builds only, with its
  // title. The probe goes back once its action ran or its alert closed.
  return (
    <AlertDialog
      request={alert}
      onDismiss={() => {
        setAlert(undefined);
        leave();
      }}
    />
  );
}
