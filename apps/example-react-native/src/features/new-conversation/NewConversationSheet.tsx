import { router, useNavigation } from "expo-router";
import {
  createContext,
  type ReactNode,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { StyleSheet } from "react-native";
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from "react-native-reanimated";

import { NewConversationStore } from "@/core/newConversation.ts";
import { newConversationQueries } from "@/features/new-conversation/sdkQueries";
import { type ActiveSession, useActiveSession } from "@/session";
import { useReducedMotion } from "@/ui/accessibility";
import { useColors } from "@/ui/colors";
import { reducedTiming } from "@/ui/motion";

interface SheetContext {
  readonly store: NewConversationStore;
  /** Closes the sheet (the xmark). */
  readonly close: () => void;
}

const Context = createContext<SheetContext | undefined>(undefined);

/** The sheet's store and close action; undefined while no session is open. */
export function useNewConversation(): SheetContext | undefined {
  return use(Context);
}

/**
 * The New message sheet around its own stack. It owns one
 * `NewConversationStore` for both steps (a fresh one per presentation, like
 * `AppGraph.navigate(.newConversation)`). Create closes the sheet and pushes
 * the new chat. With Reduce Motion the sheet appears without the slide and
 * its content cross-fades in and out over 0.2 s (`ReducedMotionSheet`).
 */
export function NewConversationSheet({
  children,
}: {
  readonly children: ReactNode;
}) {
  const session = useActiveSession();
  if (session === undefined) return null;
  return (
    <SheetBody key={session.generation} session={session}>
      {children}
    </SheetBody>
  );
}

function SheetBody({
  session,
  children,
}: {
  readonly session: ActiveSession;
  readonly children: ReactNode;
}) {
  const navigation = useNavigation();
  const palette = useColors();
  const reduceMotion = useReducedMotion();
  const opacity = useSharedValue(1);
  const closing = useRef(false);

  // The first reduced-motion frame starts transparent, then fades in.
  useEffect(() => {
    if (!reduceMotion) return;
    opacity.value = 0;
    opacity.value = withTiming(1, reducedTiming);
  }, [reduceMotion, opacity]);

  const dismiss = useCallback(
    (then?: () => void) => {
      if (closing.current) return;
      closing.current = true;
      const leave = () => {
        // The layout is a screen of the root stack: this closes the sheet.
        navigation.goBack();
        then?.();
      };
      if (!reduceMotion) {
        leave();
        return;
      }
      // Like `ReducedMotionSheet`: fade out, then dismiss without animation.
      opacity.value = withTiming(0, reducedTiming);
      setTimeout(leave, reducedTiming.duration);
    },
    [navigation, opacity, reduceMotion],
  );
  const dismissRef = useRef(dismiss);
  useEffect(() => {
    dismissRef.current = dismiss;
  }, [dismiss]);

  const [store] = useState(
    () =>
      new NewConversationStore(newConversationQueries(session), {
        admit: () => session.isCurrent(),
        finish: (conversationId) => {
          dismissRef.current(() => {
            router.push({
              pathname: "/chat/[conversationId]",
              params: { conversationId },
            });
          });
        },
      }),
  );
  useEffect(() => {
    store.load();
    return () => store.dispose();
  }, [store]);

  const value = useMemo(
    () => ({
      store,
      close: () => {
        dismiss();
      },
    }),
    [store, dismiss],
  );
  const fade = useAnimatedStyle(() => ({ opacity: opacity.value }));

  return (
    <Context value={value}>
      {!reduceMotion ? (
        children
      ) : (
        <Animated.View
          style={[
            styles.fill,
            { backgroundColor: palette.systemGroupedBackground },
            fade,
          ]}>
          {children}
        </Animated.View>
      )}
    </Context>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
});
