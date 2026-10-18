import { ActivityIndicator, StyleSheet, View } from "react-native";

import { useIsAccessibilitySize } from "@/ui/accessibility";
import { GlassSurface } from "@/ui/GlassSurface";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export interface ConnectionBannerProps {
  /** For example "Reconnecting to your server…" or "Disconnected. Retrying…". */
  text: string;
  testID?: string;
}

/** Text that means work is in progress, so the banner shows a spinner. */
function isProgress(text: string): boolean {
  return text.startsWith("Connecting") || text.startsWith("Reconnecting");
}

/**
 * The Chats connection banner: a glass capsule with a spinner or
 * `wifi.exclamationmark` and subheadline text, leading-aligned with 20 pt
 * sides and 12 pt below (`ChatsView` `safeAreaInset(edge: .top)`).
 */
export function ConnectionBanner({ text, testID }: ConnectionBannerProps) {
  const accessibilitySize = useIsAccessibilitySize();
  return (
    <View style={styles.outer} accessibilityLiveRegion="polite">
      <GlassSurface shape="capsule" testID={testID} style={styles.capsule}>
        {isProgress(text) ? (
          <ActivityIndicator size="small" />
        ) : (
          <Symbol
            name="wifi.exclamationmark"
            textStyle="subheadline"
            size={accessibilitySize ? 20 : undefined}
            scalesWithText={!accessibilitySize}
          />
        )}
        <Text variant="subheadline">{text}</Text>
      </GlassSurface>
    </View>
  );
}

const styles = StyleSheet.create({
  outer: { alignItems: "flex-start", paddingBottom: 12, paddingHorizontal: 20 },
  capsule: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
    paddingHorizontal: 14,
    paddingVertical: 10,
  },
});
