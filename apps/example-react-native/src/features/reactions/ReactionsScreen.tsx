import { Stack, router } from "expo-router";
import { useState } from "react";
import { ActivityIndicator, ScrollView, StyleSheet, View } from "react-native";

import { personInitials } from "@/core/avatar.ts";
import { reactionBadges } from "@/core/chatMessages.ts";
import {
  reactionRows,
  reactionTotal,
  removableReaction,
} from "@/core/messageInteractions.ts";
import { useSheetMessageActions } from "@/features/chat/interactions/useSheetMessageActions";
import { reactionsCopy } from "@/features/reactions/copy";
import { ReactionChips } from "@/features/reactions/ReactionChips";
import { RemoveReactionButton } from "@/features/reactions/RemoveReactionButton";
import { type ActiveSession, useActiveSession } from "@/session";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { GlassIconButton } from "@/ui/GlassButton";
import { IdentityText } from "@/ui/Identity";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

export interface ReactionsScreenProps {
  readonly conversationId: string;
  readonly messageId: string;
  /** The badge that opened the sheet: its emoji starts selected. */
  readonly emoji: string;
}

/**
 * The Reactions sheet (`ReactionsFeatureView`): its own header with a glass
 * Close button, "All N" and per-emoji glass chips, the senders of the
 * selected reactions, and "Remove my reaction" when the filter holds yours.
 */
export function ReactionsScreen(props: ReactionsScreenProps) {
  const session = useActiveSession();
  return (
    <>
      <Stack.Screen options={{ headerShown: false }} />
      {session === undefined ? null : (
        <ReactionsContent
          key={`${String(session.generation)}:${props.messageId}`}
          session={session}
          {...props}
        />
      )}
    </>
  );
}

function ReactionsContent({
  session,
  conversationId,
  messageId,
  emoji,
}: ReactionsScreenProps & { readonly session: ActiveSession }) {
  const palette = useColors();
  const { actions } = useSheetMessageActions(
    session,
    conversationId,
    messageId,
  );
  const state = useStore(actions);
  const identities = useStore(session.identities);
  const [selected, setSelected] = useState(emoji);
  const row = state.message;
  const badges =
    row === undefined ? [] : reactionBadges(row.reactions, session.inboxId);
  const listRows = reactionRows(badges, selected);
  const removable =
    row === undefined
      ? undefined
      : removableReaction(badges, selected, actions.allowed);

  return (
    // One non-collapsable root: react-native-screens gives a form sheet whose
    // container holds a scroll view directly its own scroll-view layout.
    <View style={styles.screen} collapsable={false}>
      <View style={styles.header}>
        <View style={styles.balance}>
          <GlassIconButton
            systemImage="xmark"
            shape="capsule"
            accessibilityLabel={reactionsCopy.close}
            testID="reactions.close"
            onPress={() => {
              router.back();
            }}
          />
        </View>
        <Text variant="headline" accessibilityRole="header">
          {reactionsCopy.title}
        </Text>
        <View style={styles.balance} />
      </View>
      {row !== undefined ? (
        <>
          <ReactionChips
            chips={[
              { key: "", title: reactionsCopy.all(reactionTotal(badges)) },
              ...badges.map((badge) => ({
                key: badge.emoji,
                title: `${badge.emoji} ${String(badge.count)}`,
              })),
            ]}
            selected={selected}
            onSelect={setSelected}
          />
          <View style={styles.list}>
            <ScrollView
              contentInsetAdjustmentBehavior="never"
              contentContainerStyle={styles.listContent}>
              <View
                style={[
                  styles.card,
                  { backgroundColor: palette.sheetRowFill },
                ]}>
                {listRows.map((item, index) => {
                  const handle = identities.get(item.sender)?.handle;
                  return (
                    <View
                      key={`${item.emoji}:${item.sender}:${String(index)}`}
                      style={styles.row}>
                      <Avatar
                        seed={item.sender}
                        label={personInitials(
                          handle ?? item.sender,
                          handle === undefined,
                        )}
                        size={36}
                      />
                      <IdentityText
                        inboxId={item.sender}
                        handle={handle}
                        style={styles.identity}
                        numberOfLines={1}
                      />
                      <Text>{item.emoji}</Text>
                      {index < listRows.length - 1 ? (
                        <View
                          style={[
                            styles.separator,
                            { backgroundColor: palette.separator },
                          ]}
                        />
                      ) : null}
                    </View>
                  );
                })}
              </View>
            </ScrollView>
          </View>
          {removable === undefined ? null : (
            <View style={styles.remove}>
              <RemoveReactionButton
                title={reactionsCopy.removeMine}
                onPress={() => {
                  actions.react(row, removable);
                }}
              />
            </View>
          )}
        </>
      ) : state.error === undefined ? (
        <ActivityIndicator />
      ) : (
        <Text align="center">{state.error}</Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, gap: 12 },
  header: {
    alignItems: "center",
    flexDirection: "row",
    justifyContent: "space-between",
    padding: 16,
  },
  balance: {
    alignItems: "center",
    height: 44,
    justifyContent: "center",
    width: 44,
  },
  list: { flex: 1 },
  listContent: { paddingHorizontal: 16, paddingTop: 35.5 },
  card: { borderCurve: "continuous", borderRadius: 26, overflow: "hidden" },
  row: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
    // SwiftUI list rows: 15 pt above and below the 36 pt avatar.
    paddingHorizontal: 16,
    paddingVertical: 15,
  },
  identity: { flex: 1 },
  // List separators start at the row text (16 + 36 + 8).
  separator: {
    bottom: 0,
    height: StyleSheet.hairlineWidth,
    left: 60,
    position: "absolute",
    right: 0,
  },
  remove: { paddingBottom: 16, paddingHorizontal: 16 },
});
