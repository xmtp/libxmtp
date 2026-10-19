import { type ReactNode, useState } from "react";
import {
  type StyleProp,
  StyleSheet,
  type TextLayoutEvent,
  useWindowDimensions,
  View,
  type ViewStyle,
} from "react-native";
import Svg, { Path } from "react-native-svg";

import { useColors } from "@/ui/colors";
import { Span, Text } from "@/ui/Text";
import { wrappedLineHeight } from "@/ui/typography";

export interface BubbleProps {
  isOutgoing: boolean;
  /** The chat draws a tail on every bubble. Default true. */
  showsTail?: boolean;
  /** Text content inherits the bubble's body font and color. */
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}

/** Width the tail adds on its side. */
const tailWidth = 6;

/**
 * An opaque message bubble: h14 v10 padding plus 5 pt on the tail side, a
 * continuous 18 pt body radius, and a fixed tail at the bottom outer corner.
 * Outgoing bubbles use the tint with white text; incoming ones use
 * `incomingBubble` with label text.
 */
export function Bubble({
  isOutgoing,
  showsTail = true,
  children,
  style,
  testID,
}: BubbleProps) {
  const palette = useColors();
  const fill = isOutgoing ? palette.tint : palette.incomingBubble;
  const extra = showsTail ? 5 : 0;
  return (
    <View
      testID={testID}
      style={[
        styles.bubble,
        isOutgoing
          ? { paddingLeft: 14, paddingRight: 14 + extra }
          : { paddingLeft: 14 + extra, paddingRight: 14 },
        style,
      ]}>
      <View
        style={[
          styles.body,
          { backgroundColor: fill },
          showsTail
            ? isOutgoing
              ? { left: 0, right: tailWidth }
              : { left: tailWidth, right: 0 }
            : { left: 0, right: 0 },
        ]}
      />
      {showsTail ? (
        <Svg
          width={isOutgoing ? 23 : 17}
          height={14}
          style={isOutgoing ? styles.tailOutgoing : styles.tailIncoming}>
          <Path
            d={
              isOutgoing
                ? "M3 0 Q14 13 23 13 Q9 14 0 11 Z"
                : "M14 0 Q9 13 0 13 Q8 14 17 11 Z"
            }
            fill={fill}
          />
        </Svg>
      ) : null}
      <View>{children}</View>
    </View>
  );
}

/**
 * Message text in a bubble: body, white on outgoing, label on incoming.
 * Wrapped text in React Native fills the width it is offered; SwiftUI sizes
 * it to its longest line and spaces its lines slightly wider. After the first
 * layout this narrows wrapped text to its longest line and applies SwiftUI's
 * wrapped line height.
 */
export function BubbleText({
  isOutgoing,
  children,
  italic,
}: {
  isOutgoing: boolean;
  children?: ReactNode;
  italic?: boolean;
}) {
  const { width: windowWidth, fontScale } = useWindowDimensions();
  // The narrowed width belongs to one text at one size; any change measures
  // again at the full width.
  const key =
    typeof children === "string"
      ? `${children}|${windowWidth}|${fontScale}`
      : undefined;
  const [measured, setMeasured] = useState<{ key: string; width: number }>();
  const width =
    key !== undefined && measured?.key === key ? measured.width : undefined;
  const onTextLayout = (event: TextLayoutEvent) => {
    const { lines } = event.nativeEvent;
    if (key === undefined || lines.length < 2 || width !== undefined) return;
    setMeasured({
      key,
      width: Math.ceil(Math.max(...lines.map((line) => line.width))),
    });
  };
  return (
    <Text
      color={isOutgoing ? "onTint" : "label"}
      italic={italic}
      onTextLayout={onTextLayout}
      style={
        width === undefined
          ? undefined
          : { width, lineHeight: wrappedLineHeight("body") }
      }>
      {children}
    </Text>
  );
}

export interface ReactionBadgeProps {
  emoji: string;
  count: number;
  /** The own reaction: tint fill at 15 percent and accent text. */
  isSelected?: boolean;
  testID?: string;
}

/**
 * A reaction capsule (`ReactionBadge`): caption text, h7 v4 padding, a 2 pt
 * ring in the system background. Spoken as "<emoji>, <count>".
 */
export function ReactionBadge({
  emoji,
  count,
  isSelected = false,
  testID,
}: ReactionBadgeProps) {
  const palette = useColors();
  return (
    <View
      testID={testID}
      accessible
      accessibilityLabel={`${emoji}, ${count}`}
      accessibilityState={{ selected: isSelected }}
      style={[
        styles.badge,
        {
          backgroundColor: isSelected
            ? palette.tintFill15
            : palette.incomingBubble,
          borderColor: palette.systemBackground,
        },
      ]}>
      <Text variant="caption" color={isSelected ? "accentText" : "label"}>
        {emoji}
        {count > 1 ? <Span>{` ${count}`}</Span> : null}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  bubble: { paddingVertical: 10 },
  body: {
    borderCurve: "continuous",
    borderRadius: 18,
    bottom: 0,
    position: "absolute",
    top: 0,
  },
  tailOutgoing: { bottom: -1, position: "absolute", right: 0 },
  tailIncoming: { bottom: -1, left: 0, position: "absolute" },
  badge: {
    borderRadius: 999,
    borderWidth: 2,
    marginHorizontal: -1,
    marginVertical: -1,
    paddingHorizontal: 6,
    paddingVertical: 3,
  },
});
