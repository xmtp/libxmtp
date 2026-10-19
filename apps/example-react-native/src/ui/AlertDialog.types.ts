import type { AlertButton } from "@/ui/dialogs";

export interface AlertDialogRequest {
  readonly title: string;
  readonly message?: string;
  /** In SwiftUI order; a `cancel` button goes where the system puts it. */
  readonly buttons: readonly AlertButton[];
}

export interface AlertDialogProps {
  /** The alert to show, or undefined for none. */
  request: AlertDialogRequest | undefined;
  /** The alert closed, by any button. Clear the request here. */
  onDismiss: () => void;
}
