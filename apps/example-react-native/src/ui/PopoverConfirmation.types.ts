import type { StyleProp, ViewStyle } from "react-native";

export interface PopoverConfirmationRequest {
  /** iOS shows the title only where the dialog is not a popover. */
  readonly title: string;
  readonly message?: string;
  /** The action's label, for example "Block". */
  readonly confirm: string;
  readonly destructive?: boolean;
  readonly onConfirm: () => void;
}

export interface PopoverConfirmationProps {
  /** The dialog to show, or undefined for none. */
  request: PopoverConfirmationRequest | undefined;
  /**
   * The dialog closed: after the action, a background tap or Cancel. Clear
   * the request here, but do not cancel work: on iOS this can arrive before
   * the action's `onConfirm`, so `onConfirm` must act on the request it was
   * shown with, not on state that this callback clears.
   */
  onDismiss: () => void;
  /**
   * The frame the dialog points at. SwiftUI anchors a list's
   * `.confirmationDialog` to the list, so cover the list.
   */
  style?: StyleProp<ViewStyle>;
}
