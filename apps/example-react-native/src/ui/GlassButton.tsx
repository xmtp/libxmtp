import { ActivityIndicator, Pressable, StyleSheet, View } from "react-native";

import { useColors } from "@/ui/colors";
import type {
  GlassButtonProps,
  GlassIconButtonProps,
} from "@/ui/GlassButton.types";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export type { GlassButtonProps, GlassIconButtonProps };

/**
 * Plain React Native version of the glass buttons (Android). Same metrics as
 * the SwiftUI capsules: 52 pt large, 36 pt regular.
 */
export function GlassButton({
  title,
  onPress,
  variant = "glass",
  role: _role,
  size = "regular",
  font = size === "large" ? "headline" : "body",
  systemImage,
  fullWidth = false,
  disabled = false,
  loading = false,
  testID,
  accessibilityLabel,
  style,
}: GlassButtonProps) {
  const palette = useColors();
  const prominent = variant === "glassProminent";
  const foreground = prominent ? palette.onTint : palette.tint;
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? title}
      accessibilityState={{ disabled, busy: loading }}
      style={({ pressed }) => [
        styles.capsule,
        size === "large" ? styles.large : styles.regular,
        {
          backgroundColor: prominent
            ? palette.tint
            : palette.tertiarySystemFill,
          opacity: disabled ? 0.4 : pressed ? 0.7 : 1,
        },
        fullWidth ? styles.fullWidth : undefined,
        style,
      ]}>
      {loading ? <ActivityIndicator color={foreground} /> : null}
      {systemImage === undefined ? null : (
        <Symbol name={systemImage} textStyle={font} color={foreground} />
      )}
      <Text variant={font} color={foreground} numberOfLines={1}>
        {title}
      </Text>
    </Pressable>
  );
}

export function GlassIconButton({
  systemImage,
  accessibilityLabel,
  onPress,
  variant = "glass",
  size = "regular",
  disabled = false,
  testID,
  style,
}: GlassIconButtonProps) {
  const palette = useColors();
  const prominent = variant === "glassProminent";
  const dimension = size === "large" ? 48 : 44;
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      accessibilityState={{ disabled }}
      style={({ pressed }) => [
        styles.circle,
        {
          width: dimension,
          height: dimension,
          backgroundColor: prominent
            ? palette.tint
            : palette.tertiarySystemFill,
          opacity: disabled ? 0.4 : pressed ? 0.7 : 1,
        },
        style,
      ]}>
      <View>
        <Symbol
          name={systemImage}
          textStyle="title3"
          color={prominent ? palette.onTint : palette.tint}
        />
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  capsule: {
    alignItems: "center",
    borderRadius: 999,
    flexDirection: "row",
    gap: 8,
    justifyContent: "center",
  },
  regular: { minHeight: 36, paddingHorizontal: 14 },
  large: { minHeight: 52, paddingHorizontal: 20 },
  // Fills the width in a row or a column parent.
  fullWidth: { width: "100%" },
  circle: {
    alignItems: "center",
    borderRadius: 999,
    justifyContent: "center",
  },
});
