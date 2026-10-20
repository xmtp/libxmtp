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

/** Re-centering waits for a burst of row size changes to end. */
const SETTLE_DELAY_MS = 50;
/** How long after a restore row size changes still re-center its row. */
const SETTLE_WINDOW_MS = 1000;

/**
 * The bottom-anchored timeline: the older and pending page buttons on top, a
 * date separator above the first row and at each new day, rows 10 pt apart,
 * 16 pt side and bottom padding.
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

  // Visibility before the initial scroll finished (rows at the top of the
  // page) must not become the reading anchor: hold it until `onReady`.
  const ready = useRef(false);
  const pendingVisible = useRef<readonly string[]>([]);
  const onVisibleRef = useRef(onVisible);
  onVisibleRef.current = onVisible;

  // A restored reading position scrolls its row to the center, then reports
  // what is visible: a list that did not move reports no change itself.
  const targetIndex =
    state.target === undefined
      ? -1
      : messages.findIndex((row) => row.id === state.target);
  const initialTarget = useRef(targetIndex);
  const targetRef = useRef(targetIndex);
  targetRef.current = targetIndex;
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  // Bubbles inside the SwiftUI context menu take their size a layout after
  // the list placed them, which pushes the centered row down. Until the rows
  // settle (or the user scrolls), center the row again after size changes,
  // and report what is visible only once it is centered.
  const followRef = useRef(state.atNewest);
  followRef.current = state.atNewest;
  const settleId = useRef(state.target);
  // Before `onReady` the window has not started (Infinity); 0 when settled.
  const settleUntil = useRef(initialTarget.current < 0 ? 0 : Infinity);
  const settleTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const center = useCallback(() => {
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => {
      const index = messagesRef.current.findIndex(
        (row) => row.id === settleId.current,
      );
      // Jump to latest or a scroll ended the restore.
      if (index < 0 || followRef.current || settleUntil.current === 0) return;
      void listRef.current
        ?.scrollToIndex({ index, viewPosition: 0.5, animated: false })
        .then(() => {
          if (ready.current) onVisibleRef.current(pendingVisible.current);
        });
    }, SETTLE_DELAY_MS);
  }, [listRef]);
  useEffect(() => () => clearTimeout(settleTimer.current), []);
  useEffect(() => {
    // Once per new target.
    if (state.targetRevision === 0 || targetRef.current < 0) return;
    settleId.current = messagesRef.current[targetRef.current]?.id;
    settleUntil.current = Date.now() + SETTLE_WINDOW_MS;
    center();
  }, [state.targetRevision, center]);
  const onItemSizeChanged = useCallback(() => {
    if (Date.now() < settleUntil.current) center();
  }, [center]);

  // New last row while following: scroll to it.
  const lastId = messages.at(-1)?.id;
  const previousLast = useRef(lastId);
  useEffect(() => {
    if (previousLast.current === lastId) return;
    previousLast.current = lastId;
    if (!followRef.current) return;
    scrollToLatest(listRef.current, !reduceMotion);
  }, [lastId, listRef, reduceMotion]);

  const onViewableItemsChanged = useCallback(
    ({ viewableItems }: { viewableItems: ViewToken<ChatMessage>[] }) => {
      const ids = viewableItems
        .filter((token) => token.isViewable)
        .sort((left, right) => left.index - right.index)
        .map((token) => token.item.id);
      pendingVisible.current = ids;
      if (ready.current && Date.now() >= settleUntil.current) onVisible(ids);
    },
    [onVisible],
  );
  const onReady = useCallback(() => {
    ready.current = true;
    if (settleUntil.current === Infinity) {
      settleUntil.current = Date.now() + SETTLE_WINDOW_MS;
      center();
      return;
    }
    onVisible(pendingVisible.current);
  }, [center, onVisible]);

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
      onItemSizeChanged={onItemSizeChanged}
      onScrollBeginDrag={() => {
        settleUntil.current = 0;
      }}
      onLayout={() => {
        // A list that grows (Jump to latest hides) stays at the latest row.
        if (followRef.current && ready.current) {
          scrollToLatest(listRef.current, false);
        }
      }}
      ListHeaderComponent={
        hasHeader ? <PageButtons state={state} /> : undefined
      }
      testID="chat.timeline"
    />
  );
}

/** "Older messages" and "Older pending messages". */
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
        <GlassButton
          title={chatCopy.olderPending}
          onPress={() => {
            void runtime.timeline.olderPending();
          }}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1 },
  // 27 pt under the last row: the 10 pt row spacing plus 17 pt of padding.
  content: { paddingBottom: 27, paddingHorizontal: 16 },
  spaced: { paddingTop: 10 },
  separator: { paddingBottom: 22, paddingTop: 12 },
  pageButtons: { alignItems: "center", gap: 10 },
});

/** Scrolls to the end of the list, where the newest row is. */
export function scrollToLatest(
  list: LegendListRef | null,
  animated: boolean,
): void {
  void list?.scrollToEnd({ animated });
}
