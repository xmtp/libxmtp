import { Alert, Button, Host, Rectangle, Text } from "@expo/ui/swift-ui";
import { foregroundStyle } from "@expo/ui/swift-ui/modifiers";
import { useRef } from "react";
import { StyleSheet } from "react-native";

import type {
  AlertDialogProps,
  AlertDialogRequest,
} from "@/ui/AlertDialog.types";
import { useColors } from "@/ui/colors";

export type {
  AlertDialogProps,
  AlertDialogRequest,
} from "@/ui/AlertDialog.types";

/**
 * A SwiftUI `.alert`. Unlike React Native's UIAlertController, its Cancel
 * and default buttons take the app tint, as in the iOS example. Render it
 * after the screen's first view; it is invisible and lets touches through.
 */
export function AlertDialog({ request, onDismiss }: AlertDialogProps) {
  const palette = useColors();
  // Keep the last request while the alert animates out.
  const shown = useRef<AlertDialogRequest | undefined>(request);
  if (request !== undefined) shown.current = request;
  const current = shown.current;
  return (
    <Host
      pointerEvents="none"
      seedColor={palette.tint}
      style={StyleSheet.absoluteFill}>
      <Alert
        title={current?.title ?? ""}
        isPresented={request !== undefined}
        onIsPresentedChange={(presented) => {
          if (!presented) onDismiss();
        }}>
        <Alert.Trigger>
          <Rectangle modifiers={[foregroundStyle("#00000000")]} />
        </Alert.Trigger>
        <Alert.Actions>
          {current?.buttons.map((button) => (
            <Button
              key={button.text}
              label={button.text}
              role={
                button.style === "cancel" || button.style === "destructive"
                  ? button.style
                  : undefined
              }
              onPress={button.onPress}
            />
          ))}
        </Alert.Actions>
        {current?.message === undefined ? null : (
          <Alert.Message>
            <Text>{current.message}</Text>
          </Alert.Message>
        )}
      </Alert>
    </Host>
  );
}
