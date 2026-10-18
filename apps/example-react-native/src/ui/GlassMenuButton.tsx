import { Alert } from "react-native";

import { commonCopy } from "@/copy/common";
import { GlassIconButton } from "@/ui/GlassButton";
import type {
  GlassMenuButtonProps,
  GlassMenuItem,
} from "@/ui/GlassMenuButton.types";

export type { GlassMenuButtonProps, GlassMenuItem };

/** Android: an icon button that lists the items in an alert. */
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
        Alert.alert(accessibilityLabel, undefined, [
          ...items.map((item) => ({ text: item.title, onPress: item.onPress })),
          { text: commonCopy.cancel, style: "cancel" as const },
        ]);
      }}
    />
  );
}
