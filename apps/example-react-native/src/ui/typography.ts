import { Platform, type TextProps, type TextStyle } from "react-native";

type DynamicTypeRamp = NonNullable<TextProps["dynamicTypeRamp"]>;

interface TextStyleSpec {
  fontSize: number;
  fontWeight?: TextStyle["fontWeight"];
  /** The iOS Dynamic Type ramp that scales this style (`Text` prop). */
  ramp: DynamicTypeRamp;
}

/**
 * SwiftUI text styles at the default ("Large") Dynamic Type size. Line
 * heights are left to the font, like SwiftUI `Text`, and letter spacing is
 * left to SF Pro.
 */
export const textStyles = {
  largeTitle: { fontSize: 34, ramp: "largeTitle" },
  title: { fontSize: 28, ramp: "title1" },
  title2: { fontSize: 22, ramp: "title2" },
  title3: { fontSize: 20, ramp: "title3" },
  headline: { fontSize: 17, fontWeight: "600", ramp: "headline" },
  body: { fontSize: 17, ramp: "body" },
  callout: { fontSize: 16, ramp: "callout" },
  subheadline: { fontSize: 15, ramp: "subheadline" },
  footnote: { fontSize: 13, ramp: "footnote" },
  caption: { fontSize: 12, ramp: "caption1" },
  caption2: { fontSize: 11, ramp: "caption2" },
} as const satisfies Record<string, TextStyleSpec>;

export type TextVariant = keyof typeof textStyles;

/** The font of a text style, for `TextInput` and other non-`Text` views. */
export function fontOf(variant: TextVariant): TextStyle {
  const spec: TextStyleSpec = textStyles[variant];
  return spec.fontWeight === undefined
    ? { fontSize: spec.fontSize }
    : { fontSize: spec.fontSize, fontWeight: spec.fontWeight };
}

/**
 * SwiftUI advances wrapped lines about 4 percent more than the font's
 * natural line height (measured on iOS 27: body 21.17 pt, footnote 15.85 pt),
 * while a single line keeps its natural height. React Native's `lineHeight`
 * applies to every line, so set it only on text that has wrapped.
 */
export function wrappedLineHeight(variant: TextVariant): number {
  return Math.round(textStyles[variant].fontSize * 124.5) / 100;
}

export const fontWeights = {
  regular: "400",
  medium: "500",
  semibold: "600",
  bold: "700",
} as const satisfies Record<string, TextStyle["fontWeight"]>;

export type FontWeight = keyof typeof fontWeights;

/** SF Mono on iOS (`.fontDesign(.monospaced)`), the system mono font on Android. */
export const monospacedFamily = Platform.select({
  ios: "ui-monospace",
  default: "monospace",
});
