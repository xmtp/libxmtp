import { useMemo } from "react";
import {
  type StyleProp,
  StyleSheet,
  useWindowDimensions,
  View,
  type ViewStyle,
} from "react-native";

import { avatarPaletteIndex } from "@/core/avatar.ts";
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
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

/**
 * A seeded color circle with initials (`DesignSystem/Identity.swift` `Avatar`).
 * The initials are `.headline` at every size; the circle grows with Dynamic
 * Type (`@ScaledMetric(relativeTo: .body)`). Hidden from accessibility.
 */
export function Avatar({
  seed,
  label,
  size = 44,
  monospaced = false,
  style,
  testID,
}: AvatarProps) {
  const { fontScale } = useWindowDimensions();
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
        numberOfLines={1}>
        {label}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  circle: { alignItems: "center", justifyContent: "center" },
});
