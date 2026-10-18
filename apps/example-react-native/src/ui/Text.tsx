import {
  Text as NativeText,
  type TextProps as NativeTextProps,
  type TextStyle,
} from "react-native";

import { type ColorInput, resolveColor, useColors } from "@/ui/colors";
import {
  type FontWeight,
  fontWeights,
  fontOf,
  monospacedFamily,
  textStyles,
  type TextVariant,
} from "@/ui/typography";

export interface SpanProps extends NativeTextProps {
  /** A palette name or any color. */
  color?: ColorInput;
  /** Overrides the style's weight (`.weight(.semibold)`, `.bold()`). */
  weight?: FontWeight;
  /** `.fontDesign(.monospaced)`. */
  monospaced?: boolean;
  italic?: boolean;
  align?: TextStyle["textAlign"];
}

export interface TextProps extends SpanProps {
  /** SwiftUI text style. Default `body`. */
  variant?: TextVariant;
}

function useSpanStyle({
  color,
  weight,
  monospaced,
  italic,
  align,
}: SpanProps): TextStyle {
  const palette = useColors();
  const resolved = resolveColor(palette, color);
  return {
    ...(resolved === undefined ? {} : { color: resolved }),
    ...(weight === undefined ? {} : { fontWeight: fontWeights[weight] }),
    ...(monospaced === true ? { fontFamily: monospacedFamily } : {}),
    ...(italic === true ? { fontStyle: "italic" } : {}),
    ...(align === undefined ? {} : { textAlign: align }),
  };
}

/**
 * SwiftUI-styled text: a text style (default `body`) in `label` color. It
 * scales with Dynamic Type on iOS and keeps font scaling on with no maximum.
 */
export function Text({
  variant = "body",
  color = "label",
  weight,
  monospaced,
  italic,
  align,
  style,
  ...props
}: TextProps) {
  const spanStyle = useSpanStyle({ color, weight, monospaced, italic, align });
  return (
    <NativeText
      dynamicTypeRamp={textStyles[variant].ramp}
      {...props}
      style={[fontOf(variant), spanStyle, style]}
    />
  );
}

/**
 * A run inside a `Text` that inherits the parent's style and overrides only
 * what it sets (SwiftUI `Text` concatenation).
 */
export function Span({
  color,
  weight,
  monospaced,
  italic,
  align,
  style,
  ...props
}: SpanProps) {
  const spanStyle = useSpanStyle({ color, weight, monospaced, italic, align });
  return <NativeText {...props} style={[spanStyle, style]} />;
}
