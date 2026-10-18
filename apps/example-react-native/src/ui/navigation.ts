import type { NativeStackNavigationOptions } from "expo-router";
import {
  DarkTheme,
  DefaultTheme,
  type Theme,
} from "expo-router/react-navigation";
import { useMemo } from "react";
import { useColorScheme } from "react-native";

import { useColors } from "@/ui/colors";

/**
 * The navigation theme. The native header takes its light or dark style from
 * `theme.dark` (expo/expo#50357), and screens default to the system
 * background (white or black) instead of React Navigation's #F2F2F2.
 */
export function useNavigationTheme(): Theme {
  const scheme = useColorScheme();
  return useMemo(() => {
    const dark = scheme === "dark";
    const base = dark ? DarkTheme : DefaultTheme;
    return {
      ...base,
      colors: {
        ...base.colors,
        primary: dark ? "#4361D1" : "#3454BC",
        background: dark ? "#000000" : "#FFFFFF",
        card: dark ? "#000000" : "#FFFFFF",
      },
    };
  }, [scheme]);
}

/**
 * Stack defaults that match the iOS 26 SwiftUI bars: the glass back button
 * shows only the chevron, and bar glyphs use the label color (toolbar items
 * that SwiftUI tints set `tintColor` themselves).
 */
export const stackScreenOptions = {
  headerBackButtonDisplayMode: "minimal",
} as const satisfies NativeStackNavigationOptions;

export function useStackScreenOptions(): NativeStackNavigationOptions {
  const palette = useColors();
  return useMemo(
    () => ({ ...stackScreenOptions, headerTintColor: palette.label }),
    [palette],
  );
}
