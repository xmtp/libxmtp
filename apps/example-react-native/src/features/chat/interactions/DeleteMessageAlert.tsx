import { useMemo } from "react";

import { commonCopy } from "@/copy/common";
import { MessageMenuCopy } from "@/core/messageInteractions.ts";
import type { ChatRuntime } from "@/features/chat/chatRuntime";
import { AlertDialog } from "@/ui/AlertDialog";
import { useStore } from "@/ui/useStore";

/**
 * The "Delete message?" alert while `deleteTarget` is set: a SwiftUI alert on
 * iOS, so its buttons take the app tint.
 */
export function DeleteMessageAlert({
  runtime,
}: {
  readonly runtime: ChatRuntime;
}) {
  const target = useStore(runtime.actions).deleteTarget;
  const request = useMemo(
    () =>
      target === undefined
        ? undefined
        : {
            title: MessageMenuCopy.deleteTitle,
            message: MessageMenuCopy.deleteMessage,
            buttons: [
              {
                text: MessageMenuCopy.deleteConfirm,
                style: "destructive" as const,
                // The dismissal may clear the target first: delete the row
                // this alert showed.
                onPress: () => {
                  runtime.actions.deleteMessage(target);
                },
              },
              { text: commonCopy.cancel, style: "cancel" as const },
            ],
          },
    [runtime, target],
  );
  return (
    <AlertDialog
      request={request}
      onDismiss={() => {
        runtime.actions.cancelDelete();
      }}
    />
  );
}
