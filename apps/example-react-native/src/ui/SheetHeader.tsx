import { StyleSheet, View } from "react-native";

import { Text } from "@/ui/Text";

export interface SheetHeaderProps {
  title: string;
  /** For example "Step 1 of 2". */
  subtitle?: string;
}

/**
 * A two-line principal title for sheets (New message steps): headline title
 * over a caption in the high-contrast secondary color. Use it as the screen's
 * `headerTitle`.
 */
export function SheetHeader({ title, subtitle }: SheetHeaderProps) {
  return (
    <View style={styles.stack} accessible accessibilityRole="header">
      <Text variant="headline" numberOfLines={1}>
        {title}
      </Text>
      {subtitle === undefined ? null : (
        <Text variant="caption" color="themeSecondaryLabel" numberOfLines={1}>
          {subtitle}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  stack: { alignItems: "center", gap: 2 },
});
