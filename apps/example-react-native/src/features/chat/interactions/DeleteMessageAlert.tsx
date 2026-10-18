import { useEffect } from "react";

import { commonCopy } from "@/copy/common";
import { MessageMenuCopy } from "@/core/messageInteractions.ts";
import type { DeleteMessageAlertProps } from "@/features/chat/interactions/DeleteMessageAlert.types";
import { showAlert } from "@/ui/dialogs";
import { useStore } from "@/ui/useStore";

/** "Delete message?" without SwiftUI (Android): a system alert. */
export function DeleteMessageAlert({ runtime }: DeleteMessageAlertProps) {
  const target = useStore(runtime.actions).deleteTarget;
  useEffect(() => {
    if (target === undefined) return;
    showAlert(MessageMenuCopy.deleteTitle, MessageMenuCopy.deleteMessage, [
      {
        text: MessageMenuCopy.deleteConfirm,
        style: "destructive",
        onPress: () => {
          runtime.actions.confirmDelete();
        },
      },
      {
        text: commonCopy.cancel,
        style: "cancel",
        onPress: () => {
          runtime.actions.cancelDelete();
        },
      },
    ]);
  }, [runtime, target]);
  return null;
}
