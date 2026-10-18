import { useState } from "react";

import { Text, type TextProps } from "@/ui/Text";
import { wrappedLineHeight } from "@/ui/typography";

/**
 * `Text` that takes SwiftUI's wrapped line spacing once it wraps
 * (`wrappedLineHeight`); a single line keeps the font's natural height.
 */
export function WrappedText({
  variant = "body",
  style,
  onTextLayout,
  ...props
}: TextProps) {
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
        wrapped ? { lineHeight: wrappedLineHeight(variant) } : undefined,
      ]}
    />
  );
}
