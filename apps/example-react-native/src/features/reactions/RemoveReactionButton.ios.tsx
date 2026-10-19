import { Button, Host, Text } from "@expo/ui/swift-ui";
import { buttonStyle, font, frame, tint } from "@expo/ui/swift-ui/modifiers";
import { StyleSheet } from "react-native";

import type { RemoveReactionButtonProps } from "@/features/reactions/RemoveReactionButton.types";
import { useColors } from "@/ui/colors";

/**
 * "Remove my reaction": a full-width destructive glass button tinted red, at
 * least 44 pt tall.
 */
export function RemoveReactionButton({
  title,
  onPress,
}: RemoveReactionButtonProps) {
  const palette = useColors();
  return (
    <Host matchContents={{ vertical: true }} style={styles.fill}>
      <Button
        role="destructive"
        onPress={onPress}
        modifiers={[buttonStyle("glass"), tint(palette.systemRed)]}>
        <Text
          modifiers={[
            font({ textStyle: "body" }),
            frame({ maxWidth: 100_000, minHeight: 44 }),
          ]}>
          {title}
        </Text>
      </Button>
    </Host>
  );
}

const styles = StyleSheet.create({
  fill: { width: "100%" },
});
