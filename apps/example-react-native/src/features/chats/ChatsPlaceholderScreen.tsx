import { sdkVersion } from "@xmtp/react-native-sdk";
import { Stack, router } from "expo-router";
import { useMemo } from "react";
import { ScrollView, StyleSheet } from "react-native";

import { chatsCopy } from "@/features/chats/copy";
import { launchEnvironment } from "@/platform/messengerPlatform";
import { useColors } from "@/ui/colors";
import { NavigationSubtitle } from "@/ui/NavigationSubtitle";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";

/**
 * Scaffold placeholder for the Chats screen. It proves the native pieces load:
 * the SDK native module (version), the local module (launch environment and
 * navigation subtitle), and the tinted toolbar buttons, which open the
 * Settings and New message placeholders.
 */
export function ChatsPlaceholderScreen() {
  const palette = useColors();
  const version = useMemo(() => sdkVersion(), []);
  const backendUrl = useMemo(() => launchEnvironment().backendUrl, []);
  const settingsIcon = useToolbarIcon("gearshape");
  const composeIcon = useToolbarIcon("square.and.pencil");

  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={styles.content}>
      <Stack.Screen options={{ title: chatsCopy.title }} />
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={settingsIcon}
          tintColor={palette.tint}
          accessibilityLabel={chatsCopy.settings}
          onPress={() => {
            router.push("/settings");
          }}
        />
      </Stack.Toolbar>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon={composeIcon}
          tintColor={palette.tint}
          accessibilityLabel={chatsCopy.newMessage}
          onPress={() => {
            router.push("/new-conversation");
          }}
        />
      </Stack.Toolbar>
      <NavigationSubtitle subtitle={backendUrl ?? chatsCopy.noBackend} />
      <Text testID="sdk-version">{chatsCopy.sdkVersion(version)}</Text>
      <Text testID="backend-url" variant="footnote" color="secondaryLabel">
        {backendUrl === undefined
          ? chatsCopy.noBackend
          : chatsCopy.backend(backendUrl)}
      </Text>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { gap: 8, padding: 20 },
});
