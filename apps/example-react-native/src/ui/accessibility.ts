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
 * True at the accessibility text sizes (AX1 and larger), where SwiftUI's
 * `dynamicTypeSize.isAccessibilitySize` is true. The body ramp grows from 17 pt
 * (Large) to 23 pt at xxxLarge and 28 pt at AX1.
 */
export function useIsAccessibilitySize(): boolean {
  return useWindowDimensions().fontScale >= 1.6;
}
