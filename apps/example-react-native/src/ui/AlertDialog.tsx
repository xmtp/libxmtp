import { useEffect, useRef } from "react";

import type { AlertDialogProps } from "@/ui/AlertDialog.types";
import { showAlert } from "@/ui/dialogs";

export type {
  AlertDialogProps,
  AlertDialogRequest,
} from "@/ui/AlertDialog.types";

/**
 * Android: the alert as a system alert. Android fills its button slots in
 * order (negative, then positive), so Cancel goes first.
 */
export function AlertDialog({ request, onDismiss }: AlertDialogProps) {
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    if (request === undefined) return;
    const buttons = [
      ...request.buttons.filter((button) => button.style === "cancel"),
      ...request.buttons.filter((button) => button.style !== "cancel"),
    ];
    showAlert(
      request.title,
      request.message,
      buttons.map((button) => ({
        ...button,
        onPress: () => {
          dismiss.current();
          button.onPress?.();
        },
      })),
    );
  }, [request]);
  return null;
}
