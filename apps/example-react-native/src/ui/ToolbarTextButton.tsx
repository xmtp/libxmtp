import { Stack } from "expo-router";
import { Pressable, StyleSheet } from "react-native";

import type { Palette } from "@/ui/colors";
import { Text } from "@/ui/Text";
import type {
  ToolbarTextButtonProps,
  ToolbarTextItemProps,
} from "@/ui/ToolbarTextButton.types";

export type { ToolbarTextButtonProps, ToolbarTextItemProps };

/**
 * Android: `Stack.Toolbar.Button` draws only image icons there, so a titled
 * button is a pressable label in the toolbar.
 */
export function ToolbarTextButton({
  placement,
  ...item
}: ToolbarTextButtonProps) {
  return (
    <Stack.Toolbar placement={placement}>{toolbarTextItem(item)}</Stack.Toolbar>
  );
}

/**
 * The titled button as a toolbar item, for a `Stack.Toolbar` that holds other
 * items too. A function, not a component: `Stack.Toolbar` reads only its
 * direct `Stack.Toolbar.*` children on iOS. The palette argument matches the
 * iOS signature; labels take palette names here.
 */
export function toolbarTextItem(
  { title, prominent = false, disabled = false, onPress }: ToolbarTextItemProps,
  _palette?: Palette,
) {
  return (
    <Stack.Toolbar.View key={title}>
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
  );
}

const styles = StyleSheet.create({
  button: { paddingHorizontal: 8, paddingVertical: 6 },
});
