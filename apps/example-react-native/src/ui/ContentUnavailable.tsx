import type { ReactNode } from "react";
import { type StyleProp, StyleSheet, View, type ViewStyle } from "react-native";

import { type ColorName } from "@/ui/colors";
import { Symbol, type SymbolName } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import type { TextVariant } from "@/ui/typography";

export interface ContentUnavailableProps {
  title: string;
  systemImage?: SymbolName;
  description?: string;
  /** Buttons under the description (`ContentUnavailableView` actions). */
  children?: ReactNode;
  /** Symbol color. Default `secondaryLabel`. */
  symbolColor?: ColorName;
  /** Default `title2` (bold). */
  titleVariant?: TextVariant;
  /** Default `subheadline`. */
  descriptionVariant?: TextVariant;
  /** Fills and centers in the available space. Default true. */
  fill?: boolean;
  testID?: string;
  style?: StyleProp<ViewStyle>;
}

/**
 * SwiftUI `ContentUnavailableView`: a large secondary symbol, a bold title, a
 * secondary description and optional actions, centered. The Chats empty state
 * uses the same parts with a 16 pt stack spacing.
 */
export function ContentUnavailable({
  title,
  systemImage,
  description,
  children,
  symbolColor = "secondaryLabel",
  titleVariant = "title2",
  descriptionVariant = "subheadline",
  fill = true,
  testID,
  style,
}: ContentUnavailableProps) {
  return (
    <View
      testID={testID}
      style={[styles.container, fill ? styles.fill : undefined, style]}>
      {systemImage === undefined ? null : (
        <Symbol
          name={systemImage}
          textStyle="largeTitle"
          color={symbolColor}
          style={styles.symbol}
        />
      )}
      <Text
        variant={titleVariant}
        weight="bold"
        align="center"
        accessibilityRole="header">
        {title}
      </Text>
      {description === undefined ? null : (
        <Text
          variant={descriptionVariant}
          color="secondaryLabel"
          align="center">
          {description}
        </Text>
      )}
      {children === undefined ? null : (
        <View style={styles.actions}>{children}</View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    alignItems: "center",
    gap: 16,
    paddingHorizontal: 16,
    paddingVertical: 48,
  },
  fill: { flexGrow: 1, justifyContent: "center" },
  symbol: { marginBottom: 0 },
  actions: { alignItems: "center", gap: 12, marginTop: 4 },
});
