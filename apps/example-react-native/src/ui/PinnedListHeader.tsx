import { type ReactNode, useState } from "react";
import {
  Animated,
  type LayoutChangeEvent,
  Platform,
  StyleSheet,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { useColors } from "@/ui/colors";

/** The inline navigation bar under the status bar on iPhone. */
const NAVIGATION_BAR_HEIGHT = 54;

export interface PinnedListHeaderProps {
  children?: ReactNode;
  onLayout: (event: LayoutChangeEvent) => void;
  scrollAnimatedValue: Animated.Value;
}

/**
 * A `StickyHeaderComponent` that pins a plain list's section header under
 * the collapsed navigation bar, like SwiftUI. React Native's own sticky
 * header pins at the scroll view's top edge, which is behind the bar when
 * the list adjusts its insets for it (iOS). A hairline shows under it while it is
 * pinned. Give the children an opaque background.
 */
export function PinnedListHeader({
  children,
  onLayout,
  scrollAnimatedValue,
}: PinnedListHeaderProps) {
  const palette = useColors();
  const insets = useSafeAreaInsets();
  const [layoutY, setLayoutY] = useState(0);
  // The content offset at which the header reaches the bar's bottom edge.
  // Only an iOS list scrolls under the bar; on Android it starts below it.
  const barBottom =
    Platform.OS === "ios" ? insets.top + NAVIGATION_BAR_HEIGHT : 0;
  const pinAt = layoutY - barBottom;
  const translateY = scrollAnimatedValue.interpolate({
    inputRange: [pinAt, pinAt + 1],
    outputRange: [0, 1],
    extrapolateLeft: "clamp",
    extrapolateRight: "extend",
  });
  const pinned = scrollAnimatedValue.interpolate({
    inputRange: [pinAt, pinAt + 1],
    outputRange: [0, 1],
    extrapolate: "clamp",
  });
  return (
    <Animated.View
      collapsable={false}
      onLayout={(event) => {
        setLayoutY(event.nativeEvent.layout.y);
        onLayout(event);
      }}
      style={[styles.header, { transform: [{ translateY }] }]}>
      {children}
      <Animated.View
        pointerEvents="none"
        style={[
          styles.hairline,
          { backgroundColor: palette.separator, opacity: pinned },
        ]}
      />
    </Animated.View>
  );
}

const styles = StyleSheet.create({
  header: { zIndex: 10 },
  hairline: {
    bottom: 0,
    height: StyleSheet.hairlineWidth,
    left: 0,
    position: "absolute",
    right: 0,
  },
});
