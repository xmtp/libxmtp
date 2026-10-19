import { useState } from "react";

import { Text, type TextProps } from "@/ui/Text";
import {
  standardLineHeight,
  textStyles,
  type TextVariant,
  wrappedLineHeight,
} from "@/ui/typography";

export interface WrappedTextProps extends TextProps {
  /**
   * `form` (default): the tighter line pitch of List and Form rows and
   * footers. `standard`: the type ramp's leading, for text outside a List.
   */
  leading?: "form" | "standard";
}

/**
 * `Text` that takes SwiftUI's wrapped line spacing once it wraps
 * (`wrappedLineHeight` or `standardLineHeight`); a single line keeps the
 * font's natural height.
 */
export function WrappedText({
  variant = "body",
  leading = "form",
  style,
  onTextLayout,
  ...props
}: WrappedTextProps) {
  const [wrapped, setWrapped] = useState(false);
  return (
    <Text
      variant={variant}
      {...props}
      onTextLayout={(event) => {
        const next = event.nativeEvent.lines.length > 1;
        if (next !== wrapped) setWrapped(next);
        onTextLayout?.(event);
      }}
      style={[
        style,
        wrapped
          ? leading === "standard"
            ? standardSpacing(variant)
            : { lineHeight: wrappedLineHeight(variant) }
          : undefined,
      ]}
    />
  );
}

/** SF Pro's natural line height (ascender plus descender) per point of size. */
const naturalLineRatio = 1.1934;

/**
 * React Native centers each line in `lineHeight`, so the first line starts
 * lower and the last ends lower than in SwiftUI, which adds the leading only
 * between lines. Negative margins take that half-leading back off both ends.
 */
function standardSpacing(variant: TextVariant) {
  const lineHeight = standardLineHeight(variant);
  const halfLeading =
    (lineHeight - textStyles[variant].fontSize * naturalLineRatio) / 2;
  return { lineHeight, marginVertical: -halfLeading };
}
