import { Button, Host, Image, Menu } from "@expo/ui/swift-ui";
import {
  accessibilityLabel as accessibilityLabelModifier,
  buttonStyle,
  font,
  frame,
} from "@expo/ui/swift-ui/modifiers";

import { useColors } from "@/ui/colors";
import type {
  GlassMenuButtonProps,
  GlassMenuItem,
} from "@/ui/GlassMenuButton.types";

export type { GlassMenuButtonProps, GlassMenuItem };

/**
 * A SwiftUI `Menu` with a glass icon label (`.buttonStyle(.glass)`,
 * `.frame(minWidth: 44, minHeight: 44)`): the composer's "Add attachment".
 */
export function GlassMenuButton({
  systemImage,
  accessibilityLabel,
  items,
  testID,
  style,
}: GlassMenuButtonProps) {
  const palette = useColors();
  return (
    <Host matchContents seedColor={palette.tint} style={style}>
      <Menu
        testID={testID}
        label={
          <Image
            systemName={systemImage}
            modifiers={[font({ textStyle: "title3" })]}
          />
        }
        modifiers={[
          buttonStyle("glass"),
          frame({ minWidth: 44, minHeight: 44 }),
          accessibilityLabelModifier(accessibilityLabel),
        ]}>
        {items.map((item) => (
          <Button
            key={item.title}
            label={item.title}
            systemImage={item.systemImage}
            onPress={item.onPress}
          />
        ))}
      </Menu>
    </Host>
  );
}
