import { Alert, Button, Host, Text } from "@expo/ui/swift-ui";
import { StyleSheet } from "react-native";

import { commonCopy } from "@/copy/common";
import { MessageMenuCopy } from "@/core/messageInteractions.ts";
import type { DeleteMessageAlertProps } from "@/features/chat/interactions/DeleteMessageAlert.types";
import { useColors } from "@/ui/colors";
import { useStore } from "@/ui/useStore";

/**
 * `.alert("Delete message?", isPresented: deleteTarget != nil)`: a SwiftUI
 * alert, so its buttons take the app tint like the native one.
 */
export function DeleteMessageAlert({ runtime }: DeleteMessageAlertProps) {
  const palette = useColors();
  const target = useStore(runtime.actions).deleteTarget;
  return (
    <Host
      matchContents
      seedColor={palette.tint}
      style={styles.host}
      pointerEvents="none">
      <Alert
        title={MessageMenuCopy.deleteTitle}
        isPresented={target !== undefined}
        onIsPresentedChange={(presented) => {
          if (!presented) runtime.actions.cancelDelete();
        }}>
        <Alert.Trigger>
          <Text> </Text>
        </Alert.Trigger>
        <Alert.Actions>
          <Button
            label={MessageMenuCopy.deleteConfirm}
            role="destructive"
            onPress={() => {
              // The dismissal may clear the target first: delete the row
              // this alert showed.
              if (target !== undefined) runtime.actions.deleteMessage(target);
            }}
          />
          <Button label={commonCopy.cancel} role="cancel" />
        </Alert.Actions>
        <Alert.Message>
          <Text>{MessageMenuCopy.deleteMessage}</Text>
        </Alert.Message>
      </Alert>
    </Host>
  );
}

const styles = StyleSheet.create({
  host: { left: 0, opacity: 0, position: "absolute", top: 0 },
});
