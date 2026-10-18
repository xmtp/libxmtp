import {
  type ColorValue,
  DynamicColorIOS,
  Platform,
  PlatformColor,
  useColorScheme,
} from "react-native";

/**
 * A color with its light and dark values. iOS resolves brand colors with
 * `DynamicColorIOS` and system colors with `PlatformColor`, so they follow the
 * trait collection natively. Android uses the hex pair for the current scheme.
 */
interface Pair {
  light: `#${string}` | `rgb${string}`;
  dark: `#${string}` | `rgb${string}`;
  /** iOS semantic color name (`PlatformColor`). Brand colors have none. */
  ios?: string;
}

const pairs = {
  // Asset colors (`Assets.xcassets`, `DesignSystem/Theme.swift`).
  /** `AccentColor` = `Theme.tint`: global tint, outgoing bubbles, glassProminent. */
  tint: { light: "#3454BC", dark: "#4361D1" },
  /** `AccentText` = `Theme.accentText`: brand-colored text on neutral surfaces. */
  accentText: { light: "#3454BC", dark: "#91A7FF" },
  /** `SecondaryLabel` = `Theme.secondaryLabel`: opaque high-contrast secondary text. */
  themeSecondaryLabel: { light: "#5F5F67", dark: "#A3A3AB" },
  /** `IncomingBubble`: incoming bubbles, unselected reaction badges, request bar card. */
  incomingBubble: { light: "#E9E9EB", dark: "#26262A" },

  // Tints at fixed opacity (SwiftUI `Theme.tint.opacity(x)` and hierarchical styles).
  /** `.tertiary` on a tinted row (Settings chevrons): tint at 25 percent. */
  tintTertiary: { light: "rgba(52,84,188,0.25)", dark: "rgba(67,97,209,0.3)" },
  /** Recipient chips, staged attachment icon tile: tint at 10 percent. */
  tintFill10: { light: "rgba(52,84,188,0.1)", dark: "rgba(67,97,209,0.1)" },
  /** iPad sidebar selection: tint at 12 percent. */
  tintFill12: { light: "rgba(52,84,188,0.12)", dark: "rgba(67,97,209,0.12)" },
  /** Selected reaction badge: tint at 15 percent. */
  tintFill15: { light: "rgba(52,84,188,0.15)", dark: "rgba(67,97,209,0.15)" },
  /** Member role badge: system blue at 10 percent. */
  blueFill10: { light: "rgba(0,136,255,0.1)", dark: "rgba(0,145,255,0.1)" },
  /** Reply quote background: black at 8 percent in both schemes. */
  quoteFill: { light: "rgba(0,0,0,0.08)", dark: "rgba(0,0,0,0.08)" },

  // System colors. Hex values are the Android fallback (iOS 26 values where known).
  label: { light: "#000000", dark: "#FFFFFF", ios: "label" },
  secondaryLabel: {
    light: "rgba(60,60,67,0.6)",
    dark: "rgba(235,235,245,0.6)",
    ios: "secondaryLabel",
  },
  tertiaryLabel: {
    light: "rgba(60,60,67,0.3)",
    dark: "rgba(235,235,245,0.3)",
    ios: "tertiaryLabel",
  },
  quaternaryLabel: {
    light: "rgba(60,60,67,0.18)",
    dark: "rgba(235,235,245,0.16)",
    ios: "quaternaryLabel",
  },
  placeholderText: {
    light: "rgba(60,60,67,0.3)",
    dark: "rgba(235,235,245,0.3)",
    ios: "placeholderText",
  },
  systemBackground: {
    light: "#FFFFFF",
    dark: "#000000",
    ios: "systemBackground",
  },
  secondarySystemBackground: {
    light: "#F2F2F7",
    dark: "#1C1C1E",
    ios: "secondarySystemBackground",
  },
  systemGroupedBackground: {
    light: "#F2F2F7",
    dark: "#000000",
    ios: "systemGroupedBackground",
  },
  secondarySystemGroupedBackground: {
    light: "#FFFFFF",
    dark: "#1C1C1E",
    ios: "secondarySystemGroupedBackground",
  },
  tertiarySystemGroupedBackground: {
    light: "#F2F2F7",
    dark: "#2C2C2E",
    ios: "tertiarySystemGroupedBackground",
  },
  separator: {
    light: "rgba(60,60,67,0.12)",
    dark: "rgba(84,84,88,0.6)",
    ios: "separator",
  },
  opaqueSeparator: {
    light: "#C6C6C8",
    dark: "#38383A",
    ios: "opaqueSeparator",
  },
  systemFill: {
    light: "rgba(120,120,128,0.2)",
    dark: "rgba(120,120,128,0.36)",
    ios: "systemFill",
  },
  tertiarySystemFill: {
    light: "rgba(118,118,128,0.12)",
    dark: "rgba(118,118,128,0.24)",
    ios: "tertiarySystemFill",
  },
  quaternarySystemFill: {
    light: "rgba(116,116,128,0.08)",
    dark: "rgba(118,118,128,0.18)",
    ios: "quaternarySystemFill",
  },
  /** Row highlight while pressed (UITableViewCell selection). */
  rowHighlight: { light: "#D1D1D6", dark: "#3A3A3C", ios: "systemGray4" },
  systemRed: { light: "#FF3B30", dark: "#FF453A", ios: "systemRed" },
  systemGreen: { light: "#34C759", dark: "#30D158", ios: "systemGreen" },
  systemBlue: { light: "#007AFF", dark: "#0A84FF", ios: "systemBlue" },
  systemGray: { light: "#8E8E93", dark: "#8E8E93", ios: "systemGray" },
  /** Text and icons on tint-filled surfaces. */
  onTint: { light: "#FFFFFF", dark: "#FFFFFF" },
} as const satisfies Record<string, Pair>;

export type ColorName = keyof typeof pairs;
/** A concrete color: hex or rgb(a) text, or a native `PlatformColor` value. */
export type NativeColor =
  | Exclude<ColorValue, string>
  | `#${string}`
  | `rgb${string}`;
export type Palette = Readonly<Record<ColorName, NativeColor>>;

/** A color prop: a palette name or a concrete color. */
export type ColorInput = ColorName | NativeColor;

function iosColor(pair: Pair): NativeColor {
  if (pair.ios !== undefined) return PlatformColor(pair.ios);
  if (pair.light === pair.dark) return pair.light;
  // DynamicColorIOS is typed as ColorValue but returns a native color object.
  return DynamicColorIOS({ light: pair.light, dark: pair.dark }) as Exclude<
    ColorValue,
    string
  >;
}

function build(pick: (pair: Pair) => NativeColor): Palette {
  const entries = Object.entries(pairs) as [ColorName, Pair][];
  return Object.fromEntries(
    entries.map(([name, pair]) => [name, pick(pair)]),
  ) as Record<ColorName, NativeColor>;
}

const iosPalette = Platform.OS === "ios" ? build(iosColor) : undefined;
const lightPalette = build((pair) => pair.light);
const darkPalette = build((pair) => pair.dark);

/**
 * The app palette. On iOS the values are dynamic and never change identity;
 * on Android the hook re-renders with the hex values of the current scheme.
 */
export function useColors(): Palette {
  const scheme = useColorScheme();
  if (iosPalette !== undefined) return iosPalette;
  return scheme === "dark" ? darkPalette : lightPalette;
}

/**
 * The six avatar background/foreground pairs (`AvatarBackground<i>`,
 * `AvatarForeground<i>`). They are the same in light and dark mode.
 */
export const avatarPalette = [
  { background: "#DCE4FF", foreground: "#243E91" },
  { background: "#CFEDE8", foreground: "#0E5A52" },
  { background: "#FFE2CB", foreground: "#833A00" },
  { background: "#EADCFB", foreground: "#57287F" },
  { background: "#FADCE5", foreground: "#85203F" },
  { background: "#DCEFD4", foreground: "#2B5F1E" },
] as const;

/** Resolves a palette name or passes a color value through. */
export function resolveColor(
  palette: Palette,
  color: ColorInput | undefined,
): NativeColor | undefined {
  if (typeof color === "string" && color in palette) {
    return palette[color as ColorName];
  }
  return color as NativeColor | undefined;
}
