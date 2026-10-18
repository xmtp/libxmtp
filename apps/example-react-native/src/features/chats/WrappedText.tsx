import { useState } from "react";
import type { TextLayoutEvent } from "react-native";

import { Text, type TextProps } from "@/ui/Text";
import { wrappedLineHeight } from "@/ui/typography";

/**
 * Text that takes SwiftUI's wrapped line height once it wraps; a single line
 * keeps its natural height (`wrappedLineHeight`).
 */
export function WrappedText({ variant = "body", style, ...props }: TextProps) {
  const [wrapped, setWrapped] = useState(false);
  const onTextLayout = (event: TextLayoutEvent) => {
    const next = event.nativeEvent.lines.length > 1;
    if (next !== wrapped) setWrapped(next);
  };
  return (
    <Text
      {...props}
      variant={variant}
      onTextLayout={onTextLayout}
      style={[
        style,
        wrapped ? { lineHeight: wrappedLineHeight(variant) } : undefined,
      ]}
    />
  );
}
