import { ActionSheetIOS, Alert, Platform } from "react-native";

import { commonCopy } from "@/copy/common";

export interface AlertButton {
  text: string;
  style?: "default" | "cancel" | "destructive";
  onPress?: () => void;
}

/** A system alert (SwiftUI `.alert`): UIAlertController on iOS. */
export function showAlert(
  title: string,
  message?: string,
  buttons?: AlertButton[],
): void {
  Alert.alert(title, message, buttons);
}

export interface ConfirmationOptions {
  /** iOS hides a confirmation dialog's title unless it is set here. */
  title?: string;
  /**
   * The SwiftUI dialog title that iOS hides (`titleVisibility: .automatic`).
   * Only the Android alert shows it; there it wins over `confirm`.
   */
  hiddenTitle?: string;
  message?: string;
  /** The action's label, for example "Block". */
  confirm: string;
  destructive?: boolean;
  /** Default "Cancel". */
  cancel?: string;
  onConfirm: () => void;
  onCancel?: () => void;
}

/**
 * A confirmation dialog (SwiftUI `.confirmationDialog`): an action sheet on
 * iOS with the action and Cancel, an alert on Android.
 */
export function showConfirmation({
  title,
  hiddenTitle,
  message,
  confirm,
  destructive = false,
  cancel = commonCopy.cancel,
  onConfirm,
  onCancel,
}: ConfirmationOptions): void {
  if (Platform.OS === "ios") {
    ActionSheetIOS.showActionSheetWithOptions(
      {
        title,
        message,
        options: [confirm, cancel],
        destructiveButtonIndex: destructive ? 0 : undefined,
        cancelButtonIndex: 1,
      },
      (index) => {
        if (index === 0) onConfirm();
        else onCancel?.();
      },
    );
    return;
  }
  Alert.alert(title ?? hiddenTitle ?? confirm, message, [
    { text: cancel, style: "cancel", onPress: onCancel },
    {
      text: confirm,
      style: destructive ? "destructive" : "default",
      onPress: onConfirm,
    },
  ]);
}
