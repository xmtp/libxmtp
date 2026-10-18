import type { LegendListRef } from "@legendapp/list/react-native";
import {
  Stack,
  router,
  useIsFocused,
  useNavigation,
  useRoute,
} from "expo-router";
import { CommonActions, useHeaderHeight } from "expo-router/react-navigation";
import {
  type ComponentRef,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { AppState, Platform, StyleSheet, View } from "react-native";
import { useReanimatedKeyboardAnimation } from "react-native-keyboard-controller";
import Animated, { useAnimatedStyle } from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { TimelineCopy } from "@/core/chatHistory.ts";
import type { DecisionOutcome } from "@/core/requestDecisions.ts";
import { conversationInitials, conversationTitle } from "@/core/titles.ts";
import {
  ChatContext,
  type ChatContextValue,
} from "@/features/chat/ChatContext";
import { ChatHeader } from "@/features/chat/ChatHeader";
import { ChatRuntime } from "@/features/chat/chatRuntime";
import { Composer } from "@/features/chat/Composer";
import { chatCopy } from "@/features/chat/copy";
import { ChatInteractionsLayer } from "@/features/chat/interactions/ChatInteractionsLayer";
import { MessageList, scrollToLatest } from "@/features/chat/MessageList";
import { RequestBar } from "@/features/chat/RequestBar";
import { useRequestDecisions } from "@/features/chats/useConversationStores";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { GlassButton } from "@/ui/GlassButton";
import { useMotion } from "@/ui/motion";
import { Text } from "@/ui/Text";
import { useStore } from "@/ui/useStore";

/** The chat route (`ChatFeatureView`): one runtime per session and conversation. */
export function ChatScreen({ conversationId }: { conversationId: string }) {
  const palette = useColors();
  const session = useActiveSession();
  if (session === undefined) {
    return (
      <View
        style={[styles.screen, { backgroundColor: palette.systemBackground }]}
      />
    );
  }
  return (
    <ChatSessionScreen
      key={`${String(session.generation)}:${conversationId}`}
      session={session}
      conversationId={conversationId}
    />
  );
}

function ChatSessionScreen({
  session,
  conversationId,
}: {
  session: ActiveSession;
  conversationId: string;
}) {
  const palette = useColors();
  const motion = useMotion();
  const [runtime] = useState(() => new ChatRuntime(session, conversationId));
  useEffect(() => runtime.attach(), [runtime]);
  const state = useStore(runtime.timeline);
  const listRef = useRef<LegendListRef>(null);
  const composerRef = useRef<ComponentRef<typeof View>>(null);
  const [context] = useState<ChatContextValue>(() => ({
    runtime,
    conversationId,
    composerRef,
  }));

  // Visibility drives the reading anchor and the read marker. The chat is
  // "current" while focused (not behind a sheet or a pushed screen).
  const focused = useIsFocused();
  const focusedRef = useRef(focused);
  focusedRef.current = focused;
  const lastVisible = useRef<readonly string[]>([]);
  const report = useCallback(
    (ids: readonly string[]) => {
      lastVisible.current = ids;
      runtime.timeline.visible(ids, {
        foreground: AppState.currentState === "active",
        current: focusedRef.current,
      });
    },
    [runtime],
  );
  useEffect(() => {
    const subscription = AppState.addEventListener("change", (next) => {
      if (next === "active") report(lastVisible.current);
    });
    return () => {
      subscription.remove();
    };
  }, [report]);
  // Back on top (from Settings › Send needs review, Details or a sheet):
  // re-read the draft and the journal, and report what is visible.
  useEffect(() => {
    if (!focused) return;
    runtime.composer.reload();
    report(lastVisible.current);
  }, [focused, report, runtime]);

  // The request bar's decisions (`RequestsStore`). Accept leaves Chats below
  // the chat (`setPath([.chat(id)])`); Delete request and Block return to
  // Chats (`replace(.chats)`). Both only while the chat is on top.
  const navigation = useNavigation();
  const routeKey = useRoute().key;
  const onOutcome = useCallback(
    (outcome: DecisionOutcome) => {
      if (outcome.kind === "openChat") void runtime.timeline.refresh();
      if (!navigation.isFocused()) return;
      if (outcome.kind === "chats") {
        router.dismissTo("/chats");
        return;
      }
      const stack = navigation.getState();
      if (stack === undefined) return;
      const index = stack.routes.findIndex((route) => route.key === routeKey);
      if (index <= 1) return;
      // Drops the screens between Chats and this chat (Requests, Settings)
      // and keeps this chat mounted.
      const routes = [stack.routes[0], stack.routes[index]];
      navigation.dispatch({
        ...CommonActions.reset({ ...stack, routes, index: 1 }),
        target: stack.key,
      });
    },
    [navigation, routeKey, runtime],
  );
  const decisions = useRequestDecisions(session, onOutcome);

  // The bottom bar sits on the keyboard, else above the home indicator.
  const insets = useSafeAreaInsets();
  const keyboard = useReanimatedKeyboardAnimation();
  const bottom = insets.bottom;
  const columnStyle = useAnimatedStyle(() => ({
    paddingBottom: Math.max(-keyboard.height.value, bottom),
  }));

  const conversation = state.conversation;
  const alias = (() => {
    try {
      return session.preferences.alias(conversationId);
    } catch {
      return undefined;
    }
  })();
  const title =
    conversation === undefined
      ? chatCopy.fallbackTitle
      : conversationTitle(conversation, alias);
  const initials =
    conversation === undefined ? "" : conversationInitials(conversation, title);
  const memberCount =
    conversation?.kind === "group" ? conversation.memberCount : undefined;
  const isRequest = conversation?.consent === "unknown";
  const showsJump = !state.atNewest || !state.window.atNewest;
  // The notice and error rows sit between the navigation bar and the list
  // (the list then no longer scrolls under the bar).
  const banner = state.window.notice !== undefined || state.error !== undefined;
  const headerHeight = useHeaderHeight();

  return (
    <ChatContext value={context}>
      <Stack.Screen
        options={{
          title,
          headerTitle: () => (
            <ChatHeader
              conversationId={conversationId}
              title={title}
              initials={initials}
              {...(memberCount === undefined ? {} : { memberCount })}
              onPress={() => {
                router.push({
                  pathname: "/details/[conversationId]",
                  params: { conversationId },
                });
              }}
            />
          ),
        }}
      />
      <Animated.View
        style={[
          styles.screen,
          { backgroundColor: palette.systemBackground },
          columnStyle,
        ]}>
        {banner && Platform.OS === "ios" ? (
          // The screen starts under the translucent iOS bar; Android's is opaque.
          <View style={{ height: headerHeight }} />
        ) : null}
        {state.window.notice === undefined ? null : (
          <Text
            variant="caption"
            align="center"
            style={styles.notice}
            testID="chat.notice">
            {state.window.notice}
          </Text>
        )}
        {state.error === undefined ? null : (
          <View style={styles.error}>
            <Text variant="caption" testID="chat.error">
              {state.error}
            </Text>
            {state.error === TimelineCopy.noSurvivors ? null : (
              <GlassButton
                title={chatCopy.retry}
                font="caption"
                testID="chat.retry"
                onPress={() => {
                  void runtime.timeline.refresh();
                }}
              />
            )}
          </View>
        )}
        {state.loaded ? (
          <MessageList
            state={state}
            listRef={listRef}
            onVisible={report}
            underHeader={!banner}
          />
        ) : (
          <View style={styles.screen} />
        )}
        {showsJump ? (
          <GlassButton
            title={chatCopy.jumpToLatest}
            systemImage="arrow.down"
            testID="chat.jump-to-latest"
            style={styles.jump}
            onPress={() => {
              void runtime.timeline.newest();
              scrollToLatest(listRef.current, !motion.reduceMotion);
            }}
          />
        ) : null}
        {isRequest ? (
          <Animated.View entering={motion.entering} exiting={motion.exiting}>
            <RequestBar decisions={decisions} />
          </Animated.View>
        ) : (
          <Composer />
        )}
      </Animated.View>
      <ChatInteractionsLayer />
    </ChatContext>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  notice: { padding: 8 },
  error: {
    alignItems: "center",
    flexDirection: "row",
    gap: 8,
    justifyContent: "center",
    padding: 8,
  },
  jump: { alignSelf: "center", margin: 8 },
});
