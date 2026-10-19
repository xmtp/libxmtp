import { Stack, router, useNavigation } from "expo-router";
import { useCallback, useMemo, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  View,
} from "react-native";

import type { ConversationRow } from "@/core/conversationList.ts";
import type {
  DecisionOutcome,
  RequestDecision,
} from "@/core/requestDecisions.ts";
import {
  ConversationListRow,
  useSwipeGroup,
} from "@/features/chats/ConversationListRow";
import {
  useConversationList,
  useRequestDecisions,
  useVisibleRowsReporter,
} from "@/features/chats/useConversationStores";
import { requestsCopy } from "@/features/requests/copy";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { PinnedListHeader } from "@/ui/PinnedListHeader";
import { PopoverConfirmation } from "@/ui/PopoverConfirmation";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";
import { WrappedText } from "@/ui/WrappedText";

/** The load error and Retry, above the rows in the same section. */
const ERROR_ITEM = { id: "requests.error" } as const;
type RequestsItem = ConversationRow | typeof ERROR_ITEM;
const isRow = (item: RequestsItem): item is ConversationRow =>
  item !== ERROR_ITEM;

const chatHref = (id: string) =>
  ({
    pathname: "/chat/[conversationId]",
    params: { conversationId: id },
  }) as const;

/**
 * Message requests: conversations with unknown consent, with Accept, Delete
 * (local only) and Block.
 */
export function RequestsScreen() {
  const palette = useColors();
  const session = useActiveSession();
  return (
    <>
      <Stack.Screen options={{ title: requestsCopy.title }} />
      {session === undefined ? (
        <View
          style={[styles.fill, { backgroundColor: palette.systemBackground }]}
        />
      ) : (
        <RequestsList key={session.generation} session={session} />
      )}
    </>
  );
}

function confirmationRequest(
  decision: RequestDecision,
  confirm: (decision: RequestDecision) => void,
) {
  const block = decision.kind === "block";
  return {
    title: block ? requestsCopy.blockTitle : requestsCopy.deleteTitle,
    message: block ? requestsCopy.blockDetail : requestsCopy.deleteDetail,
    confirm: block ? requestsCopy.block : requestsCopy.delete,
    destructive: true,
    onConfirm: () => {
      confirm(decision);
    },
  };
}

function RequestsList({ session }: { readonly session: ActiveSession }) {
  const palette = useColors();
  const navigation = useNavigation();
  const store = useConversationList(session, "unknown");
  const state = useStore(store);

  const onOutcome = useCallback(
    (outcome: DecisionOutcome) => {
      // Only while Requests is on top: a late outcome must not navigate.
      if (!navigation.isFocused()) return;
      if (outcome.kind === "chats") {
        router.dismissTo("/chats");
        return;
      }
      // Accept: the stack becomes Chats → Chat (`setPath([.chat(id)])`).
      const routes = navigation.getState()?.routes ?? [];
      if (routes.at(-2)?.name === "chats") {
        router.replace(chatHref(outcome.id));
      } else {
        router.dismissTo("/chats");
        router.push(chatHref(outcome.id));
      }
    },
    [navigation],
  );
  const decisions = useRequestDecisions(session, onOutcome);
  const decisionState = useStore(decisions);
  const swipeGroup = useSwipeGroup();
  const [refreshing, setRefreshing] = useState(false);
  const onViewableItemsChanged = useVisibleRowsReporter(store);

  const onRefresh = useCallback(() => {
    setRefreshing(true);
    void store.refresh().finally(() => {
      setRefreshing(false);
    });
  }, [store]);

  const retry = useCallback(() => {
    store.load();
  }, [store]);

  const renderItem = useCallback(
    ({ item }: { item: RequestsItem }) => {
      if (!isRow(item)) {
        return (
          <LoadError
            error={state.error ?? ""}
            loading={state.loading}
            onRetry={retry}
          />
        );
      }
      const accept = {
        label: requestsCopy.accept,
        onPress: () => {
          decisions.accept(item.id);
        },
      };
      const remove = {
        label: requestsCopy.delete,
        destructive: true,
        onPress: () => {
          decisions.requestDelete(item.id);
        },
      };
      const block = {
        label: requestsCopy.block,
        destructive: true,
        onPress: () => {
          decisions.requestBlock(item.id);
        },
      };
      return (
        <ConversationListRow
          row={item}
          testIDPrefix="requests.row"
          menu={[accept, remove, block]}
          swipe={[remove, block]}
          swipeGroup={swipeGroup}
          rowBackground="system"
        />
      );
    },
    [decisions, swipeGroup, retry, state.error, state.loading],
  );

  const items = useMemo<readonly RequestsItem[]>(
    () =>
      state.error === undefined
        ? state.visible
        : [ERROR_ITEM, ...state.visible],
    [state.error, state.visible],
  );

  const confirmation = decisionState.confirmation;
  const request = useMemo(
    () =>
      confirmation === undefined
        ? undefined
        : confirmationRequest(confirmation, (decision) => {
            decisions.confirm(decision);
          }),
    [confirmation, decisions],
  );

  return (
    <>
      <FlatList
        data={items}
        keyExtractor={(item) => item.id}
        renderItem={renderItem}
        contentInsetAdjustmentBehavior="automatic"
        // The route's first view, so the large title collapses with it.
        style={[styles.fill, { backgroundColor: palette.systemBackground }]}
        contentContainerStyle={styles.content}
        onScrollBeginDrag={() => {
          swipeGroup.closeAll();
        }}
        onViewableItemsChanged={onViewableItemsChanged}
        viewabilityConfig={viewability}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={onRefresh} />
        }
        // A plain list's section header stays under the navigation bar.
        stickyHeaderIndices={[0]}
        StickyHeaderComponent={PinnedListHeader}
        ListHeaderComponent={
          <WrappedText
            color="secondaryLabel"
            style={[
              styles.header,
              { backgroundColor: palette.systemBackground },
            ]}>
            {requestsCopy.detail}
          </WrappedText>
        }
        ListEmptyComponent={
          <View style={styles.empty}>
            {state.loading || !state.loaded ? <ActivityIndicator /> : null}
            <Text variant="headline" align="center">
              {state.loading || !state.loaded
                ? requestsCopy.loading
                : requestsCopy.emptyTitle}
            </Text>
            {state.loading || !state.loaded ? null : (
              <WrappedText color="secondaryLabel" align="center">
                {requestsCopy.emptyDetail}
              </WrappedText>
            )}
          </View>
        }
        ListFooterComponent={
          <>
            <View style={styles.footer}>
              <WrappedText>{requestsCopy.footer}</WrappedText>
              <View
                style={[
                  styles.footerLine,
                  { backgroundColor: palette.separator },
                ]}
              />
            </View>
            {decisionState.error === undefined ? null : (
              <Text
                style={styles.decisionError}
                testID="requests.decisionError">
                {decisionState.error}
              </Text>
            )}
          </>
        }
      />
      <PopoverConfirmation
        request={request}
        onDismiss={() => {
          decisions.cancel();
        }}
      />
    </>
  );
}

const viewability = { itemVisiblePercentThreshold: 1 };

/** The load error with Retry (disabled while a load runs). */
function LoadError({
  error,
  loading,
  onRetry,
}: {
  readonly error: string;
  readonly loading: boolean;
  readonly onRetry: () => void;
}) {
  return (
    <View style={styles.error}>
      <Text color="secondaryLabel" align="center" testID="requests.error">
        {error}
      </Text>
      <Pressable
        accessibilityRole="button"
        disabled={loading}
        onPress={onRetry}>
        <Text color={loading ? "tertiaryLabel" : "accentText"}>
          {requestsCopy.retry}
        </Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  content: { paddingBottom: 24 },
  header: { paddingBottom: 10, paddingHorizontal: 16, paddingTop: 10 },
  empty: {
    alignItems: "center",
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 40,
  },
  error: {
    alignItems: "center",
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 24,
  },
  footer: { paddingHorizontal: 16, paddingTop: 14.5 },
  footerLine: { height: 1, marginTop: 14.5 },
  decisionError: { paddingHorizontal: 16, paddingVertical: 12 },
});
