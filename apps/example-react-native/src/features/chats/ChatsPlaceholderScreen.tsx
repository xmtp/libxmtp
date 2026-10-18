import { Stack, router } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, ScrollView, StyleSheet, View } from "react-native";

import { ChatsPlaceholderStore } from "@/features/chats/chatsPlaceholderStore";
import { chatsCopy } from "@/features/chats/copy";
import { type ActiveSession, useActiveSession } from "@/session";
import { Avatar } from "@/ui/Avatar";
import { useColors } from "@/ui/colors";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { GlassButton } from "@/ui/GlassButton";
import { NavigationSubtitle } from "@/ui/NavigationSubtitle";
import { PlainListRow } from "@/ui/PlainListRow";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { useStore } from "@/ui/useStore";

function openNewConversation(): void {
  router.push("/new-conversation");
}

/**
 * Placeholder Chats: the large title with the server subtitle, the Settings
 * and New message toolbar buttons, and the allowed conversations, each
 * opening its chat. The chats lane replaces it with the full screen.
 */
export function ChatsPlaceholderScreen() {
  const palette = useColors();
  const session = useActiveSession();
  const settingsIcon = useToolbarIcon("gearshape");
  const composeIcon = useToolbarIcon("square.and.pencil");

  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={{ backgroundColor: palette.systemBackground }}
      contentContainerStyle={styles.content}>
      <Stack.Screen options={{ title: chatsCopy.title }} />
      {session === undefined ? null : (
        <NavigationSubtitle subtitle={session.server.name} />
      )}
      <Stack.Toolbar placement="left">
        <Stack.Toolbar.Button
          icon={settingsIcon}
          tintColor={palette.tint}
          accessibilityLabel={chatsCopy.settings}
          onPress={() => {
            router.push("/settings");
          }}
        />
      </Stack.Toolbar>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon={composeIcon}
          tintColor={palette.tint}
          accessibilityLabel={chatsCopy.newMessage}
          onPress={openNewConversation}
        />
      </Stack.Toolbar>
      {session === undefined ? null : (
        <ChatRows key={session.generation} session={session} />
      )}
    </ScrollView>
  );
}

function ChatRows({ session }: { readonly session: ActiveSession }) {
  const [store] = useState(() => new ChatsPlaceholderStore(session));
  useEffect(() => () => store.dispose(), [store]);
  const state = useStore(store);

  if (state.rows.length === 0) {
    if (state.error !== undefined) {
      return (
        <View style={styles.message}>
          <Text testID="chats.error">{chatsCopy.loadFailed}</Text>
          <GlassButton
            title={chatsCopy.retry}
            onPress={() => {
              store.reload();
            }}
          />
        </View>
      );
    }
    if (state.loading) {
      return (
        <View style={styles.message}>
          <ActivityIndicator size="large" />
          <Text variant="headline">{chatsCopy.loading}</Text>
        </View>
      );
    }
    return (
      <ContentUnavailable
        fill={false}
        style={styles.empty}
        systemImage="bubble.left.and.bubble.right"
        title={chatsCopy.emptyTitle}
        description={chatsCopy.emptyDetail}>
        <GlassButton
          variant="glassProminent"
          size="large"
          font="headline"
          title={chatsCopy.emptyAction}
          onPress={openNewConversation}
        />
      </ContentUnavailable>
    );
  }

  return state.rows.map((row, index) => (
    <PlainListRow
      key={row.id}
      testID={`chats.row.${row.id}`}
      accessibilityLabel={row.title}
      showsTopSeparator={index === 0}
      separatorInset={72}
      onPress={() => {
        router.push({
          pathname: "/chat/[conversationId]",
          params: { conversationId: row.id },
        });
      }}
      style={styles.row}>
      <Avatar seed={row.id} label={row.initials} />
      <Text variant="headline" numberOfLines={1} style={styles.title}>
        {row.title}
      </Text>
    </PlainListRow>
  ));
}

const styles = StyleSheet.create({
  content: { paddingBottom: 24 },
  empty: { marginHorizontal: 16, marginTop: 48 },
  message: { alignItems: "center", gap: 16, paddingVertical: 48 },
  row: { gap: 12, paddingVertical: 10 },
  title: { flex: 1 },
});
