import { Stack } from "expo-router";
import { ScrollView, StyleSheet, View } from "react-native";

import { galleryCopy } from "@/features/dev/copy";
import { Avatar } from "@/ui/Avatar";
import { Bubble, BubbleText, ReactionBadge } from "@/ui/Bubble";
import { useColors } from "@/ui/colors";
import { ConversationTitle } from "@/ui/Identity";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";

const copy = galleryCopy.chat;

function Row({
  isOutgoing,
  text,
  sent = false,
  reaction,
}: {
  isOutgoing: boolean;
  text: string;
  sent?: boolean;
  reaction?: string;
}) {
  return (
    <View
      style={[
        styles.row,
        isOutgoing ? styles.outgoing : styles.incoming,
        reaction === undefined ? undefined : styles.reacted,
      ]}>
      <View
        style={[
          styles.column,
          { alignItems: isOutgoing ? "flex-end" : "flex-start" },
        ]}>
        <View>
          <Bubble isOutgoing={isOutgoing}>
            <BubbleText isOutgoing={isOutgoing}>{text}</BubbleText>
          </Bubble>
          {reaction === undefined ? null : (
            <View style={styles.badges}>
              <View style={styles.badgeTarget}>
                <ReactionBadge emoji={reaction} count={1} />
              </View>
            </View>
          )}
        </View>
        {sent ? (
          <Text variant="caption2" color="secondaryLabel">
            {copy.sent}
          </Text>
        ) : null}
      </View>
    </View>
  );
}

/** Chat bubbles with sample content (a DM with lena). */
export function ChatReplica() {
  const palette = useColors();
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: palette.systemBackground }}
      contentContainerStyle={styles.content}>
      <Stack.Screen
        options={{
          headerLargeTitleEnabled: false,
          headerTitle: () => (
            <View style={styles.header}>
              <Avatar seed="marco-inbox" label="L" size={30} />
              <View style={styles.headerTitle}>
                <ConversationTitle
                  title="@lena"
                  variant="caption"
                  weight="semibold"
                />
                <Symbol
                  name="chevron.right"
                  textStyle="caption2"
                  weight="semibold"
                  color="themeSecondaryLabel"
                />
              </View>
            </View>
          ),
        }}
      />
      <Row isOutgoing={false} text={copy.thereYouGo} />
      <Row isOutgoing text={copy.welcome} sent />
      <Row isOutgoing text={copy.moveWentFine} sent />
      <Row isOutgoing text={copy.cantWait} sent />
      <Row isOutgoing text={copy.bringWine} sent reaction="🥂" />
      <View style={[styles.row, styles.incoming]}>
        <Bubble isOutgoing={false}>
          <View style={styles.reply}>
            <View
              style={[styles.quote, { backgroundColor: palette.quoteFill }]}>
              <Text variant="caption" numberOfLines={3}>
                {copy.welcome}
              </Text>
            </View>
            <BubbleText isOutgoing={false}>{copy.thankYou}</BubbleText>
          </View>
        </Bubble>
      </View>
      <Row isOutgoing={false} text={copy.perfect} />
      <View style={[styles.row, styles.incoming]}>
        <Bubble isOutgoing={false}>
          <BubbleText isOutgoing={false} italic>
            {copy.deleted}
          </BubbleText>
        </Bubble>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { gap: 10, paddingBottom: 16, paddingHorizontal: 16 },
  row: { flexDirection: "row" },
  incoming: { justifyContent: "flex-start", paddingRight: 42 },
  outgoing: { justifyContent: "flex-end", paddingLeft: 42 },
  reacted: { paddingVertical: 6 },
  column: { gap: 4 },
  // Reaction badges: 44 pt targets at the top-trailing corner, 12 pt up.
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
  reply: { gap: 6 },
  quote: { borderRadius: 8, padding: 8 },
  header: { alignItems: "center", gap: 2 },
  headerTitle: { alignItems: "center", flexDirection: "row", gap: 4 },
});
