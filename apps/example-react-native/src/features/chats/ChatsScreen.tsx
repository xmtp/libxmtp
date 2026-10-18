import { Stack, router } from "expo-router";
import { useCallback, useMemo, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  RefreshControl,
  StyleSheet,
  View,
} from "react-native";

import type { ConversationRow } from "@/core/conversationList.ts";
import {
  ConversationListRow,
  SwipeGroup,
} from "@/features/chats/ConversationListRow";
import { chatsCopy, requestCountText } from "@/features/chats/copy";
import {
  useConversationList,
  useRequestDecisions,
  useVisibleRowsReporter,
} from "@/features/chats/useConversationStores";
import { type ActiveSession, useActiveSession } from "@/session";
import { useIsAccessibilitySize } from "@/ui/accessibility";
import { useColors } from "@/ui/colors";
import { ConnectionBanner } from "@/ui/ConnectionBanner";
import { ContentUnavailable } from "@/ui/ContentUnavailable";
import { Chevron } from "@/ui/FormRows";
import { GlassButton } from "@/ui/GlassButton";
import { NavigationSubtitle } from "@/ui/NavigationSubtitle";
import { PlainListRow } from "@/ui/PlainListRow";
import { PopoverConfirmation } from "@/ui/PopoverConfirmation";
import { Symbol } from "@/ui/Symbol";
import { Text } from "@/ui/Text";
import { useToolbarIcon } from "@/ui/toolbarIcon";
import { useStore } from "@/ui/useStore";

function openNewConversation(): void {
  router.push("/new-conversation");
}

/**
 * Chats (`ChatsView`): the allowed conversations by last activity under the
 * Message requests row, with search, pull to refresh, swipe and context-menu
 * Block, and the connection banner.
 */
export function ChatsScreen() {
  const palette = useColors();
  const session = useActiveSession();
  const settingsIcon = useToolbarIcon("gearshape");
  const composeIcon = useToolbarIcon("square.and.pencil");
  return (
    <>
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
      {session === undefined ? (
        <View
          style={[styles.fill, { backgroundColor: palette.systemBackground }]}
        />
      ) : (
        <ChatsList key={session.generation} session={session} />
      )}
    </>
  );
}

function ChatsList({ session }: { readonly session: ActiveSession }) {
  const palette = useColors();
  const store = useConversationList(session, "allowed");
  const state = useStore(store);
  const decisions = useRequestDecisions(session, () => {
    // Chats is already the root (`routes.replace(.chats)`); show the change now.
    void store.refresh();
  });
  const confirmation = useStore(decisions).confirmation;
  const connection = useStore(session.streams.connection);
  const [swipeGroup] = useState(() => new SwipeGroup());
  const [refreshing, setRefreshing] = useState(false);

  const onViewableItemsChanged = useVisibleRowsReporter(store);

  const onRefresh = useCallback(() => {
    setRefreshing(true);
    void store.refresh().finally(() => {
      setRefreshing(false);
    });
  }, [store]);

  const renderItem = useCallback(
    ({ item }: { item: ConversationRow }) => {
      const block = {
        label: chatsCopy.block,
        destructive: true,
        onPress: () => {
          decisions.requestBlock(item.id);
        },
      };
      return (
        <ConversationListRow
          row={item}
          menu={[block]}
          swipe={[block]}
          swipeGroup={swipeGroup}
        />
      );
    },
    [decisions, swipeGroup],
  );

  const request = useMemo(
    () =>
      confirmation === undefined
        ? undefined
        : {
            title: chatsCopy.blockConfirm,
            confirm: chatsCopy.block,
            destructive: true,
            onConfirm: () => {
              decisions.confirm(confirmation);
            },
          },
    [confirmation, decisions],
  );

  const empty = state.visible.length === 0;
  return (
    <View style={[styles.fill, { backgroundColor: palette.systemBackground }]}>
      <FlatList
        data={state.visible}
        keyExtractor={(row) => row.id}
        renderItem={renderItem}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="on-drag"
        style={styles.fill}
        contentContainerStyle={styles.content}
        onScrollBeginDrag={() => {
          swipeGroup.closeAll();
        }}
        onViewableItemsChanged={onViewableItemsChanged}
        viewabilityConfig={viewability}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={onRefresh} />
        }
        ListHeaderComponent={
          <>
            {connection.banner === undefined ? null : (
              <ConnectionBanner
                text={connection.banner}
                testID="chats.banner"
              />
            )}
            <RequestsRow
              count={state.requestCount}
              showsSeparator={!empty || state.error !== undefined}
            />
          </>
        }
        ListEmptyComponent={
          state.error === undefined ? (
            <ChatsEmptyState
              loading={state.loading || !state.loaded}
              searching={state.search !== ""}
            />
          ) : undefined
        }
        ListFooterComponent={
          <>
            {!empty && state.loading ? (
              <View style={styles.progressRow}>
                <ActivityIndicator />
              </View>
            ) : null}
            {state.error === undefined ? null : (
              <>
                <PlainListRow style={styles.errorRow}>
                  <Text testID="chats.error">{state.error}</Text>
                </PlainListRow>
                <PlainListRow style={styles.errorRow}>
                  <GlassButton
                    title={chatsCopy.retry}
                    onPress={() => {
                      store.load();
                    }}
                  />
                </PlainListRow>
              </>
            )}
          </>
        }
      />
      <Stack.SearchBar
        placeholder={chatsCopy.search}
        onChangeText={(event) => {
          store.setSearch(event.nativeEvent.text);
        }}
        onCancelButtonPress={() => {
          store.setSearch("");
        }}
      />
      <Stack.Toolbar placement="bottom">
        <Stack.Toolbar.SearchBarSlot />
      </Stack.Toolbar>
      <PopoverConfirmation
        request={request}
        onDismiss={() => {
          decisions.cancel();
        }}
      />
    </View>
  );
}

const viewability = { itemVisiblePercentThreshold: 1 };

/** The "Message requests · N" row (`ChatsView` first row). */
function RequestsRow({
  count,
  showsSeparator,
}: {
  readonly count: number | undefined;
  readonly showsSeparator: boolean;
}) {
  const accessibilitySize = useIsAccessibilitySize();
  const countText = count === undefined ? undefined : requestCountText(count);
  return (
    <PlainListRow
      testID="chats.requests"
      showsTopSeparator
      showsSeparator={showsSeparator}
      separatorInset={56}
      accessibilityLabel={[
        chatsCopy.requests,
        countText,
        chatsCopy.requestsDetail,
      ]
        .filter((part) => part !== undefined)
        .join(", ")}
      onPress={() => {
        router.push("/requests");
      }}
      style={styles.requests}>
      <Symbol
        name="tray"
        textStyle="title3"
        color="tint"
        size={accessibilitySize ? 24 : undefined}
        scalesWithText={!accessibilitySize}
      />
      <View style={styles.requestsText}>
        <View
          style={
            accessibilitySize
              ? styles.requestsTitleStacked
              : styles.requestsTitle
          }>
          <Text variant="headline">{chatsCopy.requests}</Text>
          {countText === undefined ? null : (
            <Text variant={accessibilitySize ? "caption" : "headline"}>
              {countText}
            </Text>
          )}
        </View>
        <Text variant="subheadline" color="secondaryLabel">
          {chatsCopy.requestsDetail}
        </Text>
      </View>
      <Chevron />
    </PlainListRow>
  );
}

/** `ChatsEmptyState`: loading, no chats yet, or no search matches. */
function ChatsEmptyState({
  loading,
  searching,
}: {
  readonly loading: boolean;
  readonly searching: boolean;
}) {
  if (loading) {
    return (
      <View style={styles.loading}>
        <ActivityIndicator size="large" />
        <Text variant="headline">{chatsCopy.loading}</Text>
      </View>
    );
  }
  if (searching) {
    return (
      <ContentUnavailable
        fill={false}
        style={styles.noMatches}
        systemImage="magnifyingglass"
        symbolColor="tint"
        title={chatsCopy.noMatches}
        description={chatsCopy.noMatchesDetail}
        descriptionVariant="body"
      />
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

const styles = StyleSheet.create({
  fill: { flex: 1 },
  content: { paddingBottom: 24 },
  // `.padding(.vertical, 8)` inside the 15 pt list row inset.
  requests: { paddingVertical: 23 },
  requestsText: { flex: 1, gap: 4 },
  requestsTitle: { flexDirection: "row", flexWrap: "wrap", gap: 4 },
  requestsTitleStacked: { gap: 4 },
  // The empty state is a list row: 16 pt side and 15 pt top row insets.
  empty: { marginHorizontal: 16, marginTop: 15 },
  noMatches: { marginHorizontal: 16, marginTop: 15, paddingHorizontal: 24 },
  loading: {
    alignItems: "center",
    gap: 16,
    marginHorizontal: 16,
    marginTop: 15,
    paddingHorizontal: 16,
    paddingVertical: 48,
  },
  progressRow: { alignItems: "center", paddingVertical: 15 },
  errorRow: { minHeight: 44 },
});
