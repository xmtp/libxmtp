import { type ComponentRef, type ReactNode, useEffect, useRef } from "react";
import {
  type AccessibilityActionEvent,
  StyleSheet,
  View,
  useWindowDimensions,
} from "react-native";
import { GestureDetector, usePanGesture } from "react-native-gesture-handler";
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from "react-native-reanimated";
import { scheduleOnRN } from "react-native-worklets";

import { type ChatMessage, contentText } from "@/core/chatMessages.ts";
import {
  MessageMenuCopy,
  SWIPE_MINIMUM_DISTANCE,
  accessibilityItems,
  canReply,
  menuItems,
  showsQuickReactions,
  swipeArmed,
} from "@/core/messageInteractions.ts";
import { useChatContext } from "@/features/chat/ChatContext";
import {
  openEmojiPicker,
  runMenuItem,
} from "@/features/chat/interactions/bubbleActions";
import { flightState } from "@/features/chat/interactions/flight";
import { MessageContextMenu } from "@/features/chat/interactions/MessageContextMenu";
import { haptics } from "@/ui/haptics";
import { reducedTiming } from "@/ui/motion";
import { useStore } from "@/ui/useStore";

export interface MessageInteractionsProps {
  readonly row: ChatMessage;
  readonly mine: boolean;
  readonly allowed: boolean;
  /** The bubble with its reaction badges. */
  readonly children: ReactNode;
}

/**
 * The interactions on one bubble (`ChatMessages.swift` `messageBubble`): the
 * long-press context menu with quick reactions, swipe to reply (a light
 * haptic when it arms at 48 pt; the bubble does not move), and hiding the
 * bubble while its send flight runs.
 */
export function MessageInteractions({
  row,
  mine,
  allowed,
  children,
}: MessageInteractionsProps) {
  const { runtime, conversationId } = useChatContext();
  const flights = flightState(runtime);
  const isGroup = useStore(runtime.timeline).conversation?.kind === "group";
  const maxWidth = bubbleMaxWidth(
    useWindowDimensions().width,
    !mine && isGroup,
  );
  const flight = useStore(flights.flight);
  const own = runtime.own;
  const rowId = row.id;

  // The flight measures this view: the bubble's bounds (badges overlap it).
  const bubbleRef = useRef<ComponentRef<typeof View>>(null);
  useEffect(() => {
    const view = bubbleRef.current;
    if (view === null) return undefined;
    return flights.register(rowId, view);
  }, [flights, rowId]);

  const flying = flight?.id === rowId && flight.mode === "fly";
  const fading = flight?.id === rowId && flight.mode === "fade";
  // Reduce Motion: the sent row fades in (`.opacity` transition, 0.2 s).
  const opacity = useSharedValue(fading ? 0 : 1);
  useEffect(() => {
    if (!fading) return;
    const land = () => {
      flights.land(rowId);
    };
    opacity.value = 0;
    opacity.value = withTiming(1, reducedTiming, () => {
      scheduleOnRN(land);
    });
  }, [fading, flights, opacity, rowId]);
  const fadeStyle = useAnimatedStyle(() => ({ opacity: opacity.value }));

  // Swipe to reply: arms once per drag, replies on release while armed.
  const replyable = canReply(row, allowed);
  const armed = useRef(false);
  const pan = usePanGesture({
    activeOffsetX: [-SWIPE_MINIMUM_DISTANCE, SWIPE_MINIMUM_DISTANCE],
    failOffsetY: [-SWIPE_MINIMUM_DISTANCE, SWIPE_MINIMUM_DISTANCE],
    runOnJS: true,
    onUpdate: (event) => {
      const now = swipeArmed(event.translationX, event.translationY, replyable);
      if (now && !armed.current) haptics.lightImpact();
      armed.current = now;
    },
    onDeactivate: (event) => {
      if (armed.current && !event.canceled) runtime.startReply(rowId);
      armed.current = false;
    },
    onFinalize: () => {
      armed.current = false;
    },
  });

  return (
    <MessageContextMenu
      items={menuItems(row, own, allowed)}
      previewText={contentText(row.content)}
      quickReactions={showsQuickReactions(allowed)}
      onItem={(item) => {
        runMenuItem(runtime, rowId, item);
      }}
      onReact={(emoji) => {
        runtime.react(rowId, emoji);
      }}
      onMore={() => {
        openEmojiPicker(conversationId, rowId);
      }}>
      <GestureDetector gesture={pan}>
        <Animated.View
          ref={bubbleRef}
          collapsable={false}
          style={[{ maxWidth }, fadeStyle, flying ? styles.hidden : undefined]}>
          {children}
        </Animated.View>
      </GestureDetector>
    </MessageContextMenu>
  );
}

export interface MessageAccessibilityProps {
  readonly row: ChatMessage;
  readonly allowed: boolean;
  /** The bubble. */
  readonly children: ReactNode;
}

/**
 * The bubble as one VoiceOver element with the menu's actions
 * (`messageAccessibilityActions`): Reply, React, Copy and Delete. Attachment
 * bubbles stay a group, so their card buttons remain reachable.
 */
export function MessageAccessibility({
  row,
  allowed,
  children,
}: MessageAccessibilityProps) {
  const { runtime } = useChatContext();
  if (row.content.kind === "attachment") return children;
  const items = accessibilityItems(row, runtime.own, allowed);
  return (
    <View
      accessible
      accessibilityActions={items.map((name) => ({
        name,
        label: MessageMenuCopy[name],
      }))}
      onAccessibilityAction={(event: AccessibilityActionEvent) => {
        const item = items.find(
          (name) => name === event.nativeEvent.actionName,
        );
        if (item !== undefined) runMenuItem(runtime, row.id, item);
      }}>
      {children}
    </View>
  );
}

/**
 * The widest a bubble gets: the screen less the list's 16 pt sides, the
 * opposite side's `Spacer(minLength: 42)` and 6 pt spacing, and a group
 * sender's 24 pt avatar with its 6 pt spacing. The SwiftUI context menu
 * hosts the bubble without a width proposal, so it needs this bound to wrap.
 */
function bubbleMaxWidth(windowWidth: number, hasAvatar: boolean): number {
  return windowWidth - 32 - 48 - (hasAvatar ? 30 : 0);
}

const styles = StyleSheet.create({
  hidden: { opacity: 0 },
});
