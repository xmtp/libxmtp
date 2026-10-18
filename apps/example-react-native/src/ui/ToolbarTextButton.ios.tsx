import { Stack } from "expo-router";

import { useColors } from "@/ui/colors";
import type { ToolbarTextButtonProps } from "@/ui/ToolbarTextButton.types";

export type { ToolbarTextButtonProps };

/**
 * A titled navigation bar button (`ToolbarItem { Button("Next") }`). It
 * renders its own `Stack.Toolbar`, so a screen uses one per placement.
 */
export function ToolbarTextButton({
  title,
  placement,
  prominent = false,
  disabled = false,
  onPress,
}: ToolbarTextButtonProps) {
  const palette = useColors();
  // SwiftUI draws the prominent title in black in dark mode.
  const style = prominent
    ? ({
        variant: "prominent",
        style: { color: palette.onTintToolbar },
      } as const)
    : {};
  return (
    <Stack.Toolbar placement={placement}>
      <Stack.Toolbar.Button
        {...style}
        tintColor={palette.tint}
        accessibilityLabel={title}
        disabled={disabled}
        onPress={onPress}>
        {title}
      </Stack.Toolbar.Button>
    </Stack.Toolbar>
  );
}
