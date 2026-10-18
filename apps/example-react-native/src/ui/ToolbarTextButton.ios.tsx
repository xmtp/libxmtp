import { Stack } from "expo-router";

import { type Palette, useColors } from "@/ui/colors";
import type {
  ToolbarTextButtonProps,
  ToolbarTextItemProps,
} from "@/ui/ToolbarTextButton.types";

export type { ToolbarTextButtonProps, ToolbarTextItemProps };

/**
 * A titled navigation bar button, such as Next. It renders its own
 * `Stack.Toolbar`, so a screen uses one per placement.
 */
export function ToolbarTextButton({
  placement,
  ...item
}: ToolbarTextButtonProps) {
  const palette = useColors();
  return (
    <Stack.Toolbar placement={placement}>
      {toolbarTextItem(item, palette)}
    </Stack.Toolbar>
  );
}

/**
 * The titled button as a toolbar item, for a `Stack.Toolbar` that holds other
 * items too. A function, not a component: `Stack.Toolbar` reads only its
 * direct `Stack.Toolbar.*` children.
 */
export function toolbarTextItem(
  { title, prominent = false, disabled = false, onPress }: ToolbarTextItemProps,
  palette: Palette,
) {
  // SwiftUI draws the prominent title in black in dark mode.
  const style = prominent
    ? ({
        variant: "prominent",
        style: { color: palette.onTintToolbar },
      } as const)
    : {};
  return (
    <Stack.Toolbar.Button
      key={title}
      {...style}
      tintColor={palette.tint}
      accessibilityLabel={title}
      disabled={disabled}
      onPress={onPress}>
      {title}
    </Stack.Toolbar.Button>
  );
}
