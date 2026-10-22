import { Pressable, StyleSheet } from "react-native";

import type { RemoveRecipientButtonProps } from "@/features/new-conversation/RemoveRecipientButton.types";
import { useColors } from "@/ui/colors";
import { Symbol } from "@/ui/Symbol";

export type { RemoveRecipientButtonProps };

/** Plain React Native chip remove button (Android). */
export function RemoveRecipientButton({
  accessibilityLabel,
  onPress,
}: RemoveRecipientButtonProps) {
  const palette = useColors();
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      hitSlop={6}
      style={({ pressed }) => [
        styles.capsule,
        {
          backgroundColor: palette.tertiarySystemFill,
          opacity: pressed ? 0.7 : 1,
        },
      ]}>
      <Symbol name="xmark" textStyle="caption" color="tint" />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  capsule: {
    alignItems: "center",
    borderRadius: 999,
    height: 26,
    justifyContent: "center",
    paddingHorizontal: 12,
  },
});
