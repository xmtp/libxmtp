import { Stack } from "expo-router";
import { Pressable, StyleSheet } from "react-native";

import { Text } from "@/ui/Text";
import type { ToolbarTextButtonProps } from "@/ui/ToolbarTextButton.types";

export type { ToolbarTextButtonProps };

/**
 * Android: `Stack.Toolbar.Button` draws only image icons there, so a titled
 * button is a pressable label in the toolbar.
 */
export function ToolbarTextButton({
  title,
  placement,
  prominent = false,
  disabled = false,
  onPress,
}: ToolbarTextButtonProps) {
  return (
    <Stack.Toolbar placement={placement}>
      <Stack.Toolbar.View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={title}
          accessibilityState={{ disabled }}
          disabled={disabled}
          hitSlop={8}
          style={styles.button}
          onPress={onPress}>
          <Text
            variant={prominent ? "headline" : "body"}
            color={disabled ? "tertiaryLabel" : "tint"}>
            {title}
          </Text>
        </Pressable>
      </Stack.Toolbar.View>
    </Stack.Toolbar>
  );
}

const styles = StyleSheet.create({
  button: { paddingHorizontal: 8, paddingVertical: 6 },
});
