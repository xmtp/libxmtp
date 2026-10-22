import type { ReactNode } from "react";
import {
  type AccessibilityActionEvent,
  type AccessibilityActionInfo,
  Pressable,
  type StyleProp,
  StyleSheet,
  View,
  type ViewStyle,
} from "react-native";

import { useColors } from "@/ui/colors";

export interface PlainListRowProps {
  children?: ReactNode;
  onPress?: () => void;
  onLongPress?: () => void;
  disabled?: boolean;
  /** Draws the separator under the row. Default true. */
  showsSeparator?: boolean;
  /** Draws a separator above the row (the first row of a plain list). */
  showsTopSeparator?: boolean;
  /** Where separators start, from the screen edge. Default 16. */
  separatorInset?: number;
  /** `listRowBackground`, for example the tint at 12 percent when selected. */
  backgroundColor?: ViewStyle["backgroundColor"];
  selected?: boolean;
  testID?: string;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  /** Custom actions (SwiftUI exposes swipe and menu actions this way). */
  accessibilityActions?: readonly AccessibilityActionInfo[];
  onAccessibilityAction?: (event: AccessibilityActionEvent) => void;
  style?: StyleProp<ViewStyle>;
}

/**
 * A row of a plain `List` (Chats, Requests): full width, 16 pt sides, 15 pt
 * top and bottom (add the row's own SwiftUI padding to `style`), a 1 pt
 * separator from `separatorInset` to 16 pt before the trailing edge, and the
 * system highlight while pressed.
 */
export function PlainListRow({
  children,
  onPress,
  onLongPress,
  disabled = false,
  showsSeparator = true,
  showsTopSeparator = false,
  separatorInset = 16,
  backgroundColor,
  selected,
  testID,
  accessibilityLabel,
  accessibilityHint,
  accessibilityActions,
  onAccessibilityAction,
  style,
}: PlainListRowProps) {
  const palette = useColors();
  const separator = (edge: "top" | "bottom") => (
    <View
      pointerEvents="none"
      style={[
        styles.separator,
        edge === "top" ? styles.top : styles.bottom,
        { backgroundColor: palette.separator, left: separatorInset },
      ]}
    />
  );
  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      onLongPress={onLongPress}
      disabled={
        disabled || (onPress === undefined && onLongPress === undefined)
      }
      accessibilityRole={onPress === undefined ? undefined : "button"}
      accessibilityLabel={accessibilityLabel}
      accessibilityHint={accessibilityHint}
      accessibilityActions={accessibilityActions}
      onAccessibilityAction={onAccessibilityAction}
      accessibilityState={selected === undefined ? undefined : { selected }}
      unstable_pressDelay={60}
      style={({ pressed }) => [
        styles.row,
        { backgroundColor: pressed ? palette.rowHighlight : backgroundColor },
        style,
      ]}>
      {showsTopSeparator ? separator("top") : null}
      {children}
      {showsSeparator ? separator("bottom") : null}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  row: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
    minHeight: 52,
    paddingHorizontal: 16,
    paddingVertical: 15,
  },
  separator: { height: 1, position: "absolute", right: 16 },
  top: { top: 0 },
  bottom: { bottom: 0 },
});
