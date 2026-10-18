import { Pressable, StyleSheet, View } from "react-native";

import { chatCopy } from "@/features/chat/copy";
import { Avatar } from "@/ui/Avatar";
import { ConversationTitle } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

export interface ChatHeaderProps {
  readonly conversationId: string;
  readonly title: string;
  readonly initials: string;
  /** Groups only. */
  readonly memberCount?: number;
  readonly onPress: () => void;
}

/**
 * The principal toolbar item (`ChatHeader.swift`): a 30 pt avatar above the
 * caption-semibold title, "· N members" for groups and a chevron. Opens
 * Details. One accessibility element (`chat.details`). Navigation bar items
 * keep their default size at every text size.
 */
export function ChatHeader({
  conversationId,
  title,
  initials,
  memberCount,
  onPress,
}: ChatHeaderProps) {
  const count =
    memberCount === undefined ? undefined : chatCopy.memberCount(memberCount);
  return (
    <Pressable
      testID="chat.details"
      accessibilityRole="button"
      accessibilityLabel={count === undefined ? title : `${title}, ${count}`}
      onPress={onPress}
      style={styles.header}>
      <Avatar
        seed={conversationId}
        label={initials}
        size={30}
        scalesWithText={false}
      />
      <View style={styles.title}>
        <ConversationTitle
          title={title}
          variant="caption"
          weight="semibold"
          numberOfLines={1}
          maxFontSizeMultiplier={1}
          style={styles.titleText}
        />
        {count === undefined ? null : (
          <Text
            variant="caption"
            color="themeSecondaryLabel"
            maxFontSizeMultiplier={1}
            testID="chat.member-count">
            {count}
          </Text>
        )}
        <Symbol
          name="chevron.right"
          textStyle="caption2"
          weight="semibold"
          color="themeSecondaryLabel"
          scalesWithText={false}
        />
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  header: { alignItems: "center", gap: 2 },
  title: { alignItems: "center", flexDirection: "row", gap: 4 },
  titleText: { flexShrink: 1 },
});
