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
  SwipeGroup,
} from "@/features/chats/ConversationListRow";
import {
  useConversationList,
  useRequestDecisions,
  useVisibleRowsReporter,
} from "@/features/chats/useConversationStores";
import { WrappedText } from "@/features/chats/WrappedText";
import { requestsCopy } from "@/features/requests/copy";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { PopoverConfirmation } from "@/ui/PopoverConfirmation";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

const chatHref = (id: string) =>
  ({
    pathname: "/chat/[conversationId]",
    params: { conversationId: id },
  }) as const;

/**
 * Message requests (`RequestsView`): conversations with unknown consent, with
 * Accept, Delete (local only) and Block.
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
      // Only while Requests is on top (`RouteOwner.admit`).
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
          menu={[accept, remove, block]}
          swipe={[remove, block]}
          swipeGroup={swipeGroup}
        />
      );
    },
    [decisions, swipeGroup],
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
    <View style={[styles.fill, { backgroundColor: palette.systemBackground }]}>
      <FlatList
        data={state.error === undefined ? state.visible : []}
        keyExtractor={(row) => row.id}
        renderItem={renderItem}
        contentInsetAdjustmentBehavior="automatic"
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
          <WrappedText color="secondaryLabel" style={styles.header}>
            {requestsCopy.detail}
          </WrappedText>
        }
        ListEmptyComponent={
          state.error === undefined ? (
            <View style={styles.empty}>
              {state.loading || !state.loaded ? <ActivityIndicator /> : null}
              <Text variant="headline" align="center">
                {state.loading || !state.loaded
                  ? requestsCopy.loading
                  : requestsCopy.emptyTitle}
              </Text>
              {state.loading || !state.loaded ? null : (
                <Text color="secondaryLabel" align="center">
                  {requestsCopy.emptyDetail}
                </Text>
              )}
            </View>
          ) : (
            <View style={styles.error}>
              <Text
                color="secondaryLabel"
                align="center"
                testID="requests.error">
                {state.error}
              </Text>
              <Pressable
                accessibilityRole="button"
                disabled={state.loading}
                onPress={() => {
                  store.load();
                }}>
                <Text color={state.loading ? "tertiaryLabel" : "accentText"}>
                  {requestsCopy.retry}
                </Text>
              </Pressable>
            </View>
          )
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
    </View>
  );
}

const viewability = { itemVisiblePercentThreshold: 1 };

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
