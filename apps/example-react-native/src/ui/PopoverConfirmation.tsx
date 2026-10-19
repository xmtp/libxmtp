import { useEffect, useRef } from "react";

import { showConfirmation } from "@/ui/dialogs";
import type { PopoverConfirmationProps } from "@/ui/PopoverConfirmation.types";

export type {
  PopoverConfirmationProps,
  PopoverConfirmationRequest,
} from "@/ui/PopoverConfirmation.types";

/**
 * Android: the confirmation as a system alert with Cancel. Either button
 * closes it, so both report the dismissal, as SwiftUI does on iOS.
 */
export function PopoverConfirmation({
  request,
  onDismiss,
}: PopoverConfirmationProps) {
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    if (request === undefined) return;
    showConfirmation({
      title: request.title,
      ...(request.message === undefined ? {} : { message: request.message }),
      confirm: request.confirm,
      destructive: request.destructive ?? false,
      onConfirm: () => {
        dismiss.current();
        request.onConfirm();
      },
      onCancel: () => {
        dismiss.current();
      },
    });
  }, [request]);
  return null;
}
