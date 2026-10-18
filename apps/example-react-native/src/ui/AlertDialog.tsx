import { useEffect, useRef } from "react";

import type { AlertDialogProps } from "@/ui/AlertDialog.types";
import { showAlert } from "@/ui/dialogs";

export type {
  AlertDialogProps,
  AlertDialogRequest,
} from "@/ui/AlertDialog.types";

/** Android: the alert as a system alert. */
export function AlertDialog({ request, onDismiss }: AlertDialogProps) {
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    if (request === undefined) return;
    showAlert(
      request.title,
      request.message,
      request.buttons.map((button) => ({
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
