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
  /** The dialog closed without the action (background tap, Cancel). */
  onDismiss: () => void;
  /**
   * The frame the dialog points at. SwiftUI anchors a list's
   * `.confirmationDialog` to the list, so cover the list.
   */
  style?: StyleProp<ViewStyle>;
}
