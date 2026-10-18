import {
  LegendList,
  type LegendListRef,
  type ViewToken,
} from "@legendapp/list/react-native";
import { type RefObject, useCallback, useEffect, useMemo, useRef } from "react";
import { StyleSheet, View } from "react-native";

import { showsDateSeparator } from "@/core/chatHistory.ts";
import { type ChatMessage, messageDate } from "@/core/chatMessages.ts";
import type { TimelineState } from "@/core/chatTimeline.ts";
import { messageDateTime } from "@/core/dates.ts";
import { useChatContext } from "@/features/chat/ChatContext";
import { chatCopy } from "@/features/chat/copy";
import { MessageRow } from "@/features/chat/MessageRow";
import { useReducedMotion } from "@/ui/accessibility";
import { GlassButton } from "@/ui/GlassButton";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

export interface MessageListProps {
  readonly state: TimelineState;
  readonly listRef: RefObject<LegendListRef | null>;
  /** The visible row IDs changed (oldest first). */
  readonly onVisible: (ids: readonly string[]) => void;
  /** The list starts under the navigation bar (no notice or error row above it). */
  readonly underHeader: boolean;
}

interface RowExtra {
  readonly own: string;
  readonly isGroup: boolean;
  readonly allowed: boolean;
  readonly handles: ReadonlyMap<string, string | undefined>;
  readonly messages: readonly ChatMessage[];
  readonly hasHeader: boolean;
}

const viewability = { itemVisiblePercentThreshold: 50 } as const;

/**
 * The bottom-anchored timeline (`ChatContentView` ScrollView): the older and
 * pending page buttons on top, a date separator above the first row and at
 * each new day, rows 10 pt apart, 16 pt side and bottom padding.
 */
export function MessageList({
  state,
  listRef,
  onVisible,
  underHeader,
}: MessageListProps) {
  const { runtime, conversationId } = useChatContext();
  const reduceMotion = useReducedMotion();
  const identities = useStore(runtime.session.identities);
  const { messages, window } = state;
  const isGroup = state.conversation?.kind === "group";
  const allowed = state.conversation?.consent === "allowed";
  const hasHeader = window.hasOlder || state.morePending;

  const handles = useMemo(() => {
    const map = new Map<string, string | undefined>();
    for (const row of messages) {
      map.set(row.sender, identities.get(row.sender)?.handle);
    }
    return map;
  }, [identities, messages]);

  const extra: RowExtra = useMemo(
    () => ({
      own: runtime.own,
      isGroup,
      allowed,
      handles,
      messages,
      hasHeader,
    }),
    [runtime.own, isGroup, allowed, handles, messages, hasHeader],
  );

  const renderItem = useCallback(
    ({
      item,
      index,
      extraData,
    }: {
      item: ChatMessage;
      index: number;
      extraData: RowExtra;
    }) => {
      const previous: ChatMessage | undefined =
        index === 0 ? undefined : extraData.messages[index - 1];
      const date = messageDate(item);
      const separator = showsDateSeparator(
        previous === undefined ? undefined : messageDate(previous),
        date,
      );
      return (
        <View
          style={
            index === 0 && !extraData.hasHeader ? undefined : styles.spaced
          }>
          {separator ? (
            <Text
              variant="caption2"
              color="secondaryLabel"
              align="center"
              style={styles.separator}>
              {messageDateTime(date)}
            </Text>
          ) : null}
          <MessageRow
            row={item}
            conversationId={conversationId}
            own={extraData.own}
            isGroup={extraData.isGroup}
            allowed={extraData.allowed}
            senderHandle={extraData.handles.get(item.sender)}
          />
        </View>
      );
    },
    [conversationId],
  );

  // A restored reading position scrolls its row to the center.
  const targetIndex =
    state.target === undefined
      ? -1
      : messages.findIndex((row) => row.id === state.target);
  const initialTarget = useRef(targetIndex);
  const targetRef = useRef(targetIndex);
  targetRef.current = targetIndex;
  useEffect(() => {
    // Once per new target.
    if (state.targetRevision === 0 || targetRef.current < 0) return;
    void listRef.current?.scrollToIndex({
      index: targetRef.current,
      viewPosition: 0.5,
      animated: false,
    });
  }, [state.targetRevision, listRef]);

  // New last row while following: scroll to it (`proxy.scrollTo("latest")`).
  const lastId = messages.at(-1)?.id;
  const followRef = useRef(state.atNewest);
  followRef.current = state.atNewest;
  const previousLast = useRef(lastId);
  useEffect(() => {
    if (previousLast.current === lastId) return;
    previousLast.current = lastId;
    if (!followRef.current) return;
    void listRef.current?.scrollToEnd({ animated: !reduceMotion });
  }, [lastId, listRef, reduceMotion]);

  // Visibility before the initial scroll finished (rows at the top of the
  // page) must not become the reading anchor: hold it until `onReady`.
  const ready = useRef(false);
  const pendingVisible = useRef<readonly string[]>([]);
  const onViewableItemsChanged = useCallback(
    ({ viewableItems }: { viewableItems: ViewToken<ChatMessage>[] }) => {
      const ids = viewableItems
        .filter((token) => token.isViewable)
        .sort((left, right) => left.index - right.index)
        .map((token) => token.item.id);
      pendingVisible.current = ids;
      if (ready.current) onVisible(ids);
    },
    [onVisible],
  );
  const onReady = useCallback(() => {
    ready.current = true;
    onVisible(pendingVisible.current);
  }, [onVisible]);

  return (
    <LegendList
      ref={listRef}
      style={styles.list}
      data={messages}
      extraData={extra}
      keyExtractor={(row) => row.id}
      renderItem={renderItem}
      estimatedItemSize={48}
      alignItemsAtEnd
      {...(initialTarget.current >= 0
        ? {
            initialScrollIndex: {
              index: initialTarget.current,
              viewPosition: 0.5,
            },
          }
        : { initialScrollAtEnd: true })}
      maintainScrollAtEnd={{ on: { layout: true, itemLayout: true } }}
      maintainVisibleContentPosition
      recycleItems={false}
      contentInsetAdjustmentBehavior={underHeader ? "automatic" : "never"}
      contentContainerStyle={styles.content}
      viewabilityConfig={viewability}
      onViewableItemsChanged={onViewableItemsChanged}
      onReady={onReady}
      ListHeaderComponent={
        hasHeader ? <PageButtons state={state} /> : undefined
      }
      testID="chat.timeline"
    />
  );
}

/** "Older messages" and the pending page buttons. */
function PageButtons({ state }: { readonly state: TimelineState }) {
  const { runtime } = useChatContext();
  return (
    <View style={styles.pageButtons}>
      {state.window.hasOlder ? (
        <GlassButton
          title={chatCopy.olderMessages}
          disabled={state.loading}
          testID="chat.older"
          onPress={() => {
            void runtime.timeline.older();
          }}
        />
      ) : null}
      {state.morePending ? (
        <>
          <GlassButton
            title={chatCopy.olderPending}
            onPress={() => {
              void runtime.timeline.olderPending();
            }}
          />
          <GlassButton
            title={chatCopy.newestPending}
            onPress={() => {
              void runtime.timeline.newestPending();
            }}
          />
        </>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1 },
  content: { paddingBottom: 16, paddingHorizontal: 16 },
  spaced: { paddingTop: 10 },
  separator: { paddingBottom: 22, paddingTop: 12 },
  pageButtons: { alignItems: "center", gap: 10 },
});
