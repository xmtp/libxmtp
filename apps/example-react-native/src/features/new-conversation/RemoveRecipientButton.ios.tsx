import { Button, Host } from "@expo/ui/swift-ui";
import { buttonStyle, font, labelStyle } from "@expo/ui/swift-ui/modifiers";

import type { RemoveRecipientButtonProps } from "@/features/new-conversation/RemoveRecipientButton.types";
import { useColors } from "@/ui/colors";

export type { RemoveRecipientButtonProps };

/** A chip's remove button: `Image(systemName: "xmark").font(.caption)` in `.glass`. */
export function RemoveRecipientButton({
  accessibilityLabel,
  onPress,
}: RemoveRecipientButtonProps) {
  const palette = useColors();
  return (
    <Host matchContents seedColor={palette.tint}>
      <Button
        onPress={onPress}
        label={accessibilityLabel}
        systemImage="xmark"
        modifiers={[
          labelStyle("iconOnly"),
          buttonStyle("glass"),
          font({ textStyle: "caption" }),
        ]}
      />
    </Host>
  );
}
