import { Stack } from "expo-router";
import { ThemeProvider } from "expo-router/react-navigation";
import { LogBox, StyleSheet } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { KeyboardProvider } from "react-native-keyboard-controller";

import { routeTitles } from "@/copy/foundation";
import { SessionRouter } from "@/features/startup/SessionRouter";
import { SessionProvider } from "@/session";
import { useIsAccessibilitySize } from "@/ui/accessibility";
import { useNavigationTheme, useStackScreenOptions } from "@/ui/navigation";

// Reanimated scroll handlers log this on every scroll in Debug builds.
LogBox.ignoreLogs(["Sending `onAnimatedValueUpdate` with no listeners"]);

/**
 * The app shell: one native stack. Root switches (`connect`, `choose-handle`,
 * `chats`) use `router.replace`; detail screens push; `new-conversation`,
 * `member`, `reactions`, `emoji-picker`, `edit-group` and `add-member` are
 * sheets with their own stack (SwiftUI `.sheet { NavigationStack { } }`).
 * Pushed screens keep the iOS title mode: large after Chats, inline after
 * Chat and Details. `SessionProvider` starts the session; `SessionRouter`
 * switches the root screen when the session state changes.
 */
export default function RootLayout() {
  const theme = useNavigationTheme();
  const screenOptions = useStackScreenOptions();
  const accessibilitySize = useIsAccessibilitySize();
  return (
    <GestureHandlerRootView style={styles.root}>
      <KeyboardProvider>
        <ThemeProvider value={theme}>
          <SessionProvider>
            <SessionRouter />
            <Stack screenOptions={screenOptions}>
              <Stack.Screen name="index" options={{ headerShown: false }} />
              <Stack.Screen
                name="connect"
                options={{ headerShown: false, title: routeTitles.connect }}
              />
              <Stack.Screen
                name="choose-handle"
                options={{
                  headerShown: false,
                  title: routeTitles.chooseHandle,
                }}
              />
              <Stack.Screen name="chats" options={large(routeTitles.chats)} />
              <Stack.Screen
                name="requests"
                options={large(routeTitles.requests)}
              />
              <Stack.Screen
                name="settings"
                options={large(routeTitles.settings)}
              />
              <Stack.Screen
                name="change-handle"
                options={large(routeTitles.changeHandle)}
              />
              <Stack.Screen
                name="attachment-recovery"
                options={large(routeTitles.attachmentRecovery)}
              />
              <Stack.Screen
                name="chat/[conversationId]"
                options={inline(routeTitles.chat)}
              />
              <Stack.Screen
                name="details/[conversationId]"
                options={inline(routeTitles.details)}
              />
              <Stack.Screen
                name="permissions/[conversationId]"
                options={inline(routeTitles.permissions)}
              />
              <Stack.Screen
                name="disappearing/[conversationId]"
                options={inline(routeTitles.disappearing)}
              />
              <Stack.Screen
                name="shared-media/[conversationId]"
                options={inline(routeTitles.sharedMedia)}
              />
              <Stack.Screen
                name="group-fields/[conversationId]"
                options={inline(routeTitles.groupFields)}
              />
              <Stack.Screen
                name="my-fields/[conversationId]"
                options={inline(routeTitles.myFields)}
              />
              <Stack.Screen name="new-conversation" options={pageSheet} />
              <Stack.Screen name="edit-group" options={pageSheet} />
              <Stack.Screen
                name="member"
                options={formSheet([0.7, 1], accessibilitySize ? 1 : 0)}
              />
              <Stack.Screen name="add-member" options={formSheet([0.5, 1])} />
              <Stack.Screen name="reactions" options={formSheet([0.5, 1])} />
              <Stack.Screen name="emoji-picker" options={formSheet([0.5, 1])} />
              <Stack.Protected guard={__DEV__}>
                <Stack.Screen
                  name="dev/gallery"
                  options={large(routeTitles.gallery)}
                />
                <Stack.Screen
                  name="dev/session-probe"
                  options={inline(routeTitles.sessionProbe)}
                />
              </Stack.Protected>
            </Stack>
          </SessionProvider>
        </ThemeProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}

function large(title: string) {
  return { title, headerLargeTitleEnabled: true } as const;
}

function inline(title: string) {
  return { title, headerLargeTitleEnabled: false } as const;
}

/** SwiftUI's default sheet: a page sheet at the large detent. */
const pageSheet = { presentation: "modal", headerShown: false } as const;

/**
 * A sheet with detents (`.presentationDetents`). SwiftUI shows the grabber
 * when a sheet has more than one detent. Member starts large at accessibility
 * text sizes.
 */
function formSheet(detents: number[], initialDetentIndex = 0) {
  return {
    presentation: "formSheet",
    headerShown: false,
    sheetAllowedDetents: detents,
    sheetInitialDetentIndex: initialDetentIndex,
    sheetGrabberVisible: true,
  } as const;
}

const styles = StyleSheet.create({
  root: { flex: 1 },
});
