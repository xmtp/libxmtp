import { GlassView } from "expo-glass-effect";
import { useState } from "react";
import { type LayoutChangeEvent, StyleSheet, View } from "react-native";

import {
  useIncreasedContrast,
  useReduceTransparency,
} from "@/ui/accessibility";
import { useColors } from "@/ui/colors";
import type { GlassSurfaceProps } from "@/ui/GlassSurface.types";

export type { GlassSurfaceProps };

/**
 * Custom Liquid Glass chrome (`messengerGlass(in:)`, `DesignSystem/Glass.swift`).
 * With Reduce Transparency it draws the system background and a 1 pt stroke in
 * the label color at 20 percent (60 percent with Increase Contrast).
 */
export function GlassSurface({
  shape = "rect",
  cornerRadius = 20,
  interactive = false,
  style,
  children,
  testID,
}: GlassSurfaceProps) {
  const palette = useColors();
  const reduceTransparency = useReduceTransparency();
  const increasedContrast = useIncreasedContrast();
  const [height, setHeight] = useState(0);
  const radius = shape === "capsule" ? height / 2 : cornerRadius;
  const onLayout =
    shape === "capsule"
      ? (event: LayoutChangeEvent) => {
          setHeight(event.nativeEvent.layout.height);
        }
      : undefined;
  if (reduceTransparency) {
    return (
      <View
        testID={testID}
        onLayout={onLayout}
        style={[
          styles.fallback,
          {
            backgroundColor: palette.systemBackground,
            borderRadius: radius,
          },
          style,
        ]}>
        <View
          pointerEvents="none"
          style={[
            StyleSheet.absoluteFill,
            styles.stroke,
            {
              borderRadius: radius,
              borderColor: palette.label,
              opacity: increasedContrast ? 0.6 : 0.2,
            },
          ]}
        />
        {children}
      </View>
    );
  }
  return (
    <GlassView
      testID={testID}
      onLayout={onLayout}
      glassEffectStyle="regular"
      isInteractive={interactive}
      style={[{ borderRadius: radius }, style]}>
      {children}
    </GlassView>
  );
}

const styles = StyleSheet.create({
  fallback: { borderCurve: "continuous" },
  stroke: { borderCurve: "continuous", borderWidth: 1 },
});
