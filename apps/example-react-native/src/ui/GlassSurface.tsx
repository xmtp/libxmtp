import { useState } from "react";
import { type LayoutChangeEvent, StyleSheet, View } from "react-native";

import { useColors } from "@/ui/colors";
import type { GlassSurfaceProps } from "@/ui/GlassSurface.types";

export type { GlassSurfaceProps };

/**
 * Plain React Native glass stand-in (Android): the Reduce Transparency look of
 * the iOS glass surface, a background surface with a 1 pt stroke.
 */
export function GlassSurface({
  shape = "rect",
  cornerRadius = 20,
  style,
  children,
  testID,
}: GlassSurfaceProps) {
  const palette = useColors();
  const [height, setHeight] = useState(0);
  const radius = shape === "capsule" ? height / 2 : cornerRadius;
  return (
    <View
      testID={testID}
      onLayout={
        shape === "capsule"
          ? (event: LayoutChangeEvent) => {
              setHeight(event.nativeEvent.layout.height);
            }
          : undefined
      }
      style={[
        styles.surface,
        {
          backgroundColor: palette.secondarySystemGroupedBackground,
          borderColor: palette.separator,
          borderRadius: radius,
        },
        style,
      ]}>
      {children}
    </View>
  );
}

const styles = StyleSheet.create({
  surface: { borderWidth: StyleSheet.hairlineWidth },
});
