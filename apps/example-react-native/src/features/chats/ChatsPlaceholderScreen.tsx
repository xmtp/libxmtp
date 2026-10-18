import { sdkVersion } from "@xmtp/react-native-sdk";
import { Stack } from "expo-router";
import { useMemo } from "react";
import {
  Alert,
  Platform,
  PlatformColor,
  ScrollView,
  StyleSheet,
  Text,
} from "react-native";

import { chatsCopy } from "@/features/chats/copy";
import {
  launchEnvironment,
  NativeNavigationSubtitle,
} from "@/platform/messengerPlatform";

/**
 * Scaffold placeholder for the Chats screen. It proves the native pieces load:
 * the SDK native module (version), the local module (launch environment and
 * navigation subtitle), and a native toolbar button. SF Symbol toolbar icons
 * exist only on iOS.
 */
export function ChatsPlaceholderScreen() {
  const version = useMemo(() => sdkVersion(), []);
  const backendUrl = useMemo(() => launchEnvironment().backendUrl, []);
  const subtitle = backendUrl ?? chatsCopy.noBackend;

  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={styles.content}>
      <Stack.Screen options={{ title: chatsCopy.title }} />
      {Platform.OS === "ios" ? (
        <Stack.Toolbar placement="right">
          <Stack.Toolbar.Button
            icon="square.and.pencil"
            accessibilityLabel={chatsCopy.newMessage}
            onPress={() => {
              Alert.alert(chatsCopy.newMessage, chatsCopy.newMessagePending);
            }}
          />
        </Stack.Toolbar>
      ) : null}
      <NativeNavigationSubtitle subtitle={subtitle} style={styles.subtitle} />
      <Text testID="sdk-version" style={styles.body}>
        {chatsCopy.sdkVersion(version)}
      </Text>
      <Text testID="backend-url" style={styles.footnote}>
        {backendUrl === undefined
          ? chatsCopy.noBackend
          : chatsCopy.backend(backendUrl)}
      </Text>
    </ScrollView>
  );
}

// PlatformColor names are iOS semantic colors; Android keeps the theme default.
const styles = StyleSheet.create({
  content: { gap: 8, padding: 20 },
  subtitle: { height: 0, position: "absolute", width: 0 },
  body: {
    color: Platform.select({ ios: PlatformColor("label") }),
    fontSize: 17,
  },
  footnote: {
    color: Platform.select({ ios: PlatformColor("secondaryLabel") }),
    fontSize: 13,
  },
});
