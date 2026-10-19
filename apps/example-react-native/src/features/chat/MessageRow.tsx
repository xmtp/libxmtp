import { router } from "expo-router";
import { memo } from "react";
import { Pressable, StyleSheet, View } from "react-native";

import { personInitials } from "@/core/avatar.ts";
import {
  type ChatMessage,
  type ReactionBadgeModel,
  reactionBadges,
} from "@/core/chatMessages.ts";
import { chatCopy } from "@/features/chat/copy";
import { MessageAccessibility } from "@/features/chat/interactions/MessageInteractions";
import { MessageBubbleWrapper } from "@/features/chat/MessageBubbleWrapper";
import { MessageContent } from "@/features/chat/MessageContent";
import { Avatar } from "@/ui/Avatar";
import { Bubble, ReactionBadge } from "@/ui/Bubble";
import { IdentityText } from "@/ui/Identity";
import { Text } from "@/ui/Text";

export interface MessageRowProps {
  readonly row: ChatMessage;
  readonly conversationId: string;
  readonly own: string;
  /** Groups show the sender's avatar and name on incoming rows. */
  readonly isGroup: boolean;
  readonly allowed: boolean;
  /** The sender's handle, when known. */
  readonly senderHandle: string | undefined;
}

function deliveryLabel(row: ChatMessage): string {
  switch (row.delivery) {
    case "published":
      return chatCopy.sent;
    case "failed":
      return chatCopy.notSent;
    case "queued":
      return chatCopy.sending;
  }
}

function openReactions(
  conversationId: string,
  messageId: string,
  emoji: string,
): void {
  router.push({
    pathname: "/reactions/[conversationId]/[messageId]",
    params: { conversationId, messageId, emoji },
  });
}

/**
 * One message row: outgoing rows on the trailing side with a delivery label,
 * incoming rows on the leading side with the sender's 24 pt avatar and caption2
 * name in groups. Every bubble has a tail; reaction badges overlap its
 * top-trailing corner.
 */
export const MessageRow = memo(function MessageRow({
  row,
  conversationId,
  own,
  isGroup,
  allowed,
  senderHandle,
}: MessageRowProps) {
  const mine = row.sender === own;
  const showsSender = !mine && isGroup;
  const badges = reactionBadges(row.reactions, own);
  return (
    <View
      testID={`message.${row.id}`}
      style={[
        styles.row,
        mine ? styles.outgoing : styles.incoming,
        row.reactions.length > 0 ? styles.reacted : undefined,
      ]}>
      {showsSender ? (
        <Avatar
          seed={row.sender}
          label={personInitials(
            senderHandle ?? row.sender,
            senderHandle === undefined,
          )}
          size={24}
          monospaced={senderHandle === undefined}
        />
      ) : null}
      <View
        style={[
          styles.column,
          mine ? styles.columnOutgoing : styles.columnIncoming,
        ]}>
        {showsSender ? (
          <IdentityText
            variant="caption2"
            color="secondaryLabel"
            inboxId={row.sender}
            handle={senderHandle}
            short
          />
        ) : null}
        <MessageBubbleWrapper row={row} mine={mine} allowed={allowed}>
          <View>
            <MessageAccessibility row={row} allowed={allowed}>
              <Bubble isOutgoing={mine}>
                <MessageContent row={row} isOutgoing={mine} allowed={allowed} />
              </Bubble>
            </MessageAccessibility>
            {badges.length === 0 ? null : (
              <ReactionBadges
                badges={badges}
                onPress={(emoji) => {
                  openReactions(conversationId, row.id, emoji);
                }}
              />
            )}
          </View>
        </MessageBubbleWrapper>
        {mine ? (
          <Text
            variant="caption2"
            color="secondaryLabel"
            testID={`message.delivery.${row.id}`}>
            {deliveryLabel(row)}
          </Text>
        ) : null}
      </View>
    </View>
  );
});

/** Badges at the bubble's top-trailing corner, 12 pt up, each a 44 pt target. */
function ReactionBadges({
  badges,
  onPress,
}: {
  readonly badges: readonly ReactionBadgeModel[];
  readonly onPress: (emoji: string) => void;
}) {
  return (
    <View style={styles.badges} pointerEvents="box-none">
      {badges.map((badge) => (
        <Pressable
          key={badge.emoji}
          accessibilityRole="button"
          accessibilityLabel={`${badge.emoji}, ${String(badge.count)}`}
          accessibilityState={{ selected: badge.mine }}
          onPress={() => {
            onPress(badge.emoji);
          }}
          style={styles.badgeTarget}>
          <ReactionBadge
            emoji={badge.emoji}
            count={badge.count}
            isSelected={badge.mine}
          />
        </Pressable>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { alignItems: "flex-end", flexDirection: "row", gap: 6 },
  // `Spacer(minLength: 42)` plus the row's 6 pt spacing: bubbles are at most
  // 322 pt wide on a 402 pt screen.
  outgoing: { justifyContent: "flex-end", paddingLeft: 48 },
  incoming: { justifyContent: "flex-start", paddingRight: 48 },
  reacted: { paddingVertical: 6 },
  column: { flexShrink: 1, gap: 4 },
  columnOutgoing: { alignItems: "flex-end" },
  columnIncoming: { alignItems: "flex-start" },
  badges: {
    flexDirection: "row",
    gap: 2,
    position: "absolute",
    right: 0,
    top: -12,
  },
  badgeTarget: {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 44,
    minWidth: 44,
  },
});
