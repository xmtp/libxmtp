import { ActivityIndicator, StyleSheet, View } from "react-native";

import { startupCopy } from "@/features/startup/copy";
import { sessionController, useSessionState } from "@/session";
import { useColors } from "@/ui/colors";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { GlassButton } from "@/ui/GlassButton";

/**
 * The launch screen (`ExampleApp.swift` + `RootView` while restoring): a
 * spinner until `SessionRouter` replaces it, or "Cannot open app" with Retry
 * when local storage cannot be prepared.
 */
export function StartupScreen() {
  const state = useSessionState();
  const palette = useColors();
  return (
    <View
      style={[styles.screen, { backgroundColor: palette.systemBackground }]}>
      {state.kind === "storageFailed" ? (
        <ContentUnavailable
          title={startupCopy.storageTitle}
          systemImage="exclamationmark.triangle"
          description={startupCopy.storageMessage}>
          <GlassButton
            title={startupCopy.retry}
            testID="startup.retryStorage"
            onPress={() => void sessionController.start()}
          />
        </ContentUnavailable>
      ) : (
        <ActivityIndicator testID="startup.progress" />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { alignItems: "center", flex: 1, justifyContent: "center" },
});
