import { useEffect, useState } from "react";
import { AccessibilityInfo, Platform, useWindowDimensions } from "react-native";

type AccessibilityEvent = Parameters<
  typeof AccessibilityInfo.addEventListener
>[0];

function useAccessibilityFlag(
  read: () => Promise<boolean>,
  event: AccessibilityEvent,
): boolean {
  const [enabled, setEnabled] = useState(false);
  useEffect(() => {
    let current = true;
    void read().then(
      (value) => {
        if (current) setEnabled(value);
      },
      () => {},
    );
    const subscription = AccessibilityInfo.addEventListener(event, (value) => {
      setEnabled(value === true);
    });
    return () => {
      current = false;
      subscription.remove();
    };
  }, [read, event]);
  return enabled;
}

const readReduceMotion = () => AccessibilityInfo.isReduceMotionEnabled();
const readReduceTransparency = () =>
  AccessibilityInfo.isReduceTransparencyEnabled();

/** Reduce Motion. SwiftUI: `accessibilityReduceMotion`. */
export function useReducedMotion(): boolean {
  return useAccessibilityFlag(readReduceMotion, "reduceMotionChanged");
}

/** Reduce Transparency (iOS). SwiftUI: `accessibilityReduceTransparency`. */
export function useReduceTransparency(): boolean {
  return useAccessibilityFlag(
    readReduceTransparency,
    "reduceTransparencyChanged",
  );
}

function readIncreasedContrast(): Promise<boolean> {
  return Platform.OS === "ios"
    ? AccessibilityInfo.isDarkerSystemColorsEnabled()
    : Promise.resolve(false);
}

/** Increase Contrast (iOS). SwiftUI: `colorSchemeContrast == .increased`. */
export function useIncreasedContrast(): boolean {
  return useAccessibilityFlag(
    readIncreasedContrast,
    "darkerSystemColorsChanged",
  );
}

/**
 * React Native's iOS font scale for each content size category (its
 * `RCTFontSizeMultiplier` table) and the body text size iOS uses there.
 * `@ScaledMetric(relativeTo: .body)` scales by the body size over 17 pt,
 * which grows slower than React Native's multiplier at the accessibility
 * sizes (AX5: body 53 pt is 3.12 times, the multiplier is 3.571).
 */
const iosBodySizes: readonly (readonly [number, number])[] = [
  [0.823, 14],
  [0.882, 15],
  [0.941, 16],
  [1, 17],
  [1.118, 19],
  [1.235, 21],
  [1.353, 23],
  [1.786, 28],
  [2.143, 33],
  [2.643, 40],
  [3.143, 47],
  [3.571, 53],
];

/** The body text scale for a React Native font scale (see `useBodyScale`). */
export function bodyScaleFor(fontScale: number): number {
  if (Platform.OS !== "ios") return fontScale;
  let nearest: readonly [number, number] = [1, 17];
  for (const entry of iosBodySizes) {
    if (Math.abs(entry[0] - fontScale) < Math.abs(nearest[0] - fontScale))
      nearest = entry;
  }
  return nearest[1] / 17;
}

/**
 * The SwiftUI `@ScaledMetric(relativeTo: .body)` factor: 1 at the default
 * text size. Use it for sizes that grow with text (avatars, icons).
 */
export function useBodyScale(): number {
  return bodyScaleFor(useWindowDimensions().fontScale);
}

/**
 * True at the accessibility text sizes (AX1 and larger), where SwiftUI's
 * `dynamicTypeSize.isAccessibilitySize` is true. The body ramp grows from 17 pt
 * (Large) to 23 pt at xxxLarge and 28 pt at AX1.
 */
export function useIsAccessibilitySize(): boolean {
  return useWindowDimensions().fontScale >= 1.6;
}
