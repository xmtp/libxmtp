import { useMemo } from "react";
import {
  type LayoutChangeEvent,
  type StyleProp,
  StyleSheet,
  View,
  type ViewStyle,
} from "react-native";

import { avatarPaletteIndex } from "@/core/avatar.ts";
import { useBodyScale } from "@/ui/accessibility";
import { avatarPalette } from "@/ui/colors";
import { Text } from "@/ui/Text";

export interface AvatarProps {
  /** Picks the color pair: the first SHA-256 byte of its UTF-8, mod 6. */
  seed: string;
  /** The initials, drawn verbatim (see `groupInitials`, `personInitials`). */
  label: string;
  /** Diameter at the default text size (`avatarSizes`). Default 44. */
  size?: number;
  /** `.headline.monospaced()`, for inbox ID initials. */
  monospaced?: boolean;
  /**
   * Grow with Dynamic Type (default). False keeps the default size, like
   * items in the navigation bar (the chat header).
   */
  scalesWithText?: boolean;
  style?: StyleProp<ViewStyle>;
  testID?: string;
  /**
   * The initials' frame inside the circle. A SwiftUI list row starts its
   * separator at the first text, which is the avatar's initials.
   */
  onLabelLayout?: (event: LayoutChangeEvent) => void;
}

/**
 * A seeded color circle with initials.
 * The initials are `.headline` at every size; the circle grows with Dynamic
 * Type (`@ScaledMetric(relativeTo: .body)`). Hidden from accessibility.
 */
export function Avatar({
  seed,
  label,
  size = 44,
  monospaced = false,
  scalesWithText = true,
  style,
  testID,
  onLabelLayout,
}: AvatarProps) {
  const bodyScale = useBodyScale();
  const fontScale = scalesWithText ? bodyScale : 1;
  const colors = useMemo(() => avatarPalette[avatarPaletteIndex(seed)], [seed]);
  const diameter = size * fontScale;
  return (
    <View
      testID={testID}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={[
        styles.circle,
        {
          backgroundColor: colors.background,
          borderRadius: diameter / 2,
          height: diameter,
          width: diameter,
        },
        style,
      ]}>
      <Text
        variant="headline"
        monospaced={monospaced}
        color={colors.foreground}
        numberOfLines={1}
        onLayout={onLabelLayout}
        {...(scalesWithText ? {} : { maxFontSizeMultiplier: 1 })}>
        {label}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  circle: { alignItems: "center", justifyContent: "center" },
});
