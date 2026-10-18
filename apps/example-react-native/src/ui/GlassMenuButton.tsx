import { Alert } from "react-native";

import { commonCopy } from "@/copy/common";
import { GlassIconButton } from "@/ui/GlassButton";
import type {
  GlassMenuButtonProps,
  GlassMenuItem,
} from "@/ui/GlassMenuButton.types";

export type { GlassMenuButtonProps, GlassMenuItem };

/**
 * Android: an icon button that lists the items in an alert. An Android alert
 * shows at most three buttons, so Cancel is left out when the items fill them;
 * a tap outside or Back still dismisses it (`cancelable`).
 */
export function GlassMenuButton({
  systemImage,
  accessibilityLabel,
  items,
  testID,
  style,
}: GlassMenuButtonProps) {
  return (
    <GlassIconButton
      systemImage={systemImage}
      accessibilityLabel={accessibilityLabel}
      shape="capsule"
      testID={testID}
      style={style}
      onPress={() => {
        const buttons = items.map((item) => ({
          text: item.title,
          onPress: item.onPress,
        }));
        Alert.alert(
          accessibilityLabel,
          undefined,
          buttons.length >= 3
            ? buttons
            : [...buttons, { text: commonCopy.cancel, style: "cancel" }],
          { cancelable: true },
        );
      }}
    />
  );
}
