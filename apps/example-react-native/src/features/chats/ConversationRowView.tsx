import { useState } from "react";
import { type LayoutChangeEvent, StyleSheet, View } from "react-native";

import type { ConversationRow } from "@/core/conversationList.ts";
import { timeOfDay } from "@/core/dates.ts";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { ConversationTitle } from "@/ui/Identity";
import { Text } from "@/ui/Text";
import { WrappedText } from "@/ui/WrappedText";

/** Where the avatar starts in a plain list row. */
const ROW_LEADING = 16;

/**
 * The separator inset of a row: SwiftUI starts it at the row's first text,
 * which is the avatar's initials.
 */
export function useInitialsInset(): readonly [
  number,
  (event: LayoutChangeEvent) => void,
] {
  const [inset, setInset] = useState(ROW_LEADING + 14);
  const onLayout = (event: LayoutChangeEvent) => {
    const next = Math.round((ROW_LEADING + event.nativeEvent.layout.x) * 2) / 2;
    setInset((current) => (current === next ? current : next));
  };
  return [inset, onLayout];
}

export interface ConversationRowViewProps {
  row: ConversationRow;
  onInitialsLayout?: (event: LayoutChangeEvent) => void;
}

/**
 * A Chats or Message requests row (`ConversationRowView`): a 44 pt avatar,
 * the title and time on the first line, the two-line preview and the unread
 * badge on the second.
 */
export function ConversationRowView({
  row,
  onInitialsLayout,
}: ConversationRowViewProps) {
  const palette = useColors();
  const { record } = row;
  return (
    <>
      <Avatar
        seed={row.id}
        label={row.initials}
        onLabelLayout={onInitialsLayout}
      />
      <View style={styles.text}>
        <View style={styles.firstLine}>
          <ConversationTitle
            variant="headline"
            title={row.title}
            style={styles.shrink}
          />
          <View style={styles.spacer} />
          {record.lastActivity === undefined ? null : (
            <Text
              variant="caption"
              color="secondaryLabel"
              style={styles.shrink}>
              {timeOfDay(record.lastActivity)}
            </Text>
          )}
        </View>
        <View style={styles.secondLine}>
          <WrappedText
            variant="subheadline"
            color="secondaryLabel"
            numberOfLines={2}
            style={styles.shrink}>
            {record.preview}
          </WrappedText>
          <View style={styles.spacer} />
          {record.unread > 0 ? (
            <View style={[styles.badge, { backgroundColor: palette.tint }]}>
              <Text variant="caption2" weight="bold" color="onTint">
                {String(record.unread)}
              </Text>
            </View>
          ) : null}
        </View>
      </View>
    </>
  );
}

const styles = StyleSheet.create({
  text: { flex: 1, gap: 4 },
  firstLine: { alignItems: "baseline", flexDirection: "row" },
  secondLine: { alignItems: "center", flexDirection: "row" },
  shrink: { flexShrink: 1 },
  spacer: { flexGrow: 1, minWidth: 8 },
  badge: {
    borderRadius: 999,
    paddingHorizontal: 6,
    paddingVertical: 3,
  },
});
