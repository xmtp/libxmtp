import { Pressable, StyleSheet } from "react-native";

import type { RemoveReactionButtonProps } from "@/features/reactions/RemoveReactionButton.types";
import { useColors } from "@/ui/colors";
import { Text } from "@/ui/Text";

/** "Remove my reaction" without SwiftUI (Android): a red capsule button. */
export function RemoveReactionButton({
  title,
  onPress,
}: RemoveReactionButtonProps) {
  const palette = useColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={[styles.button, { backgroundColor: palette.tertiarySystemFill }]}>
      <Text color="systemRed" align="center">
        {title}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  button: {
    borderRadius: 22,
    justifyContent: "center",
    minHeight: 48,
    paddingHorizontal: 16,
  },
});
