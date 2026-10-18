import {
  Button,
  ConfirmationDialog,
  Host,
  Rectangle,
  Text,
} from "@expo/ui/swift-ui";
import { foregroundStyle } from "@expo/ui/swift-ui/modifiers";
import { useRef } from "react";
import { StyleSheet } from "react-native";

import { useColors } from "@/ui/colors";
import type {
  PopoverConfirmationProps,
  PopoverConfirmationRequest,
} from "@/ui/PopoverConfirmation.types";

export type {
  PopoverConfirmationProps,
  PopoverConfirmationRequest,
} from "@/ui/PopoverConfirmation.types";

/**
 * A SwiftUI `.confirmationDialog` anchored to the area this view covers. On
 * iOS 26 and later an iPhone shows it as a popover with the message and the
 * action, without the title or Cancel. The view itself is invisible and lets
 * touches through.
 */
export function PopoverConfirmation({
  request,
  onDismiss,
  style,
}: PopoverConfirmationProps) {
  const palette = useColors();
  // Keep the last request while the dialog animates out.
  const shown = useRef<PopoverConfirmationRequest | undefined>(request);
  if (request !== undefined) shown.current = request;
  const current = shown.current;
  return (
    <Host
      pointerEvents="none"
      seedColor={palette.tint}
      style={[StyleSheet.absoluteFill, style]}>
      <ConfirmationDialog
        title={current?.title ?? ""}
        isPresented={request !== undefined}
        onIsPresentedChange={(presented) => {
          if (!presented) onDismiss();
        }}>
        <ConfirmationDialog.Trigger>
          <Rectangle modifiers={[foregroundStyle("#00000000")]} />
        </ConfirmationDialog.Trigger>
        <ConfirmationDialog.Actions>
          {current === undefined ? null : (
            <Button
              label={current.confirm}
              role={current.destructive === true ? "destructive" : undefined}
              onPress={current.onConfirm}
            />
          )}
        </ConfirmationDialog.Actions>
        {current?.message === undefined ? null : (
          <ConfirmationDialog.Message>
            <Text>{current.message}</Text>
          </ConfirmationDialog.Message>
        )}
      </ConfirmationDialog>
    </Host>
  );
}
