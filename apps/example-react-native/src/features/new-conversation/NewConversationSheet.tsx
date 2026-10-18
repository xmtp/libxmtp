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
import { AccessibilityInfo, StyleSheet } from "react-native";
import Animated, {
  ReduceMotion,
  useAnimatedStyle,
  useReducedMotion as useLaunchReducedMotion,
  useSharedValue,
  withTiming,
} from "react-native-reanimated";

import { NewConversationStore } from "@/core/newConversation.ts";
import { newConversationQueries } from "@/features/new-conversation/sdkQueries";
import { type ActiveSession, useActiveSession } from "@/session";
import { useColors } from "@/ui/colors";
import { reducedTiming } from "@/ui/motion";

// Reanimated skips animations under Reduce Motion unless told not to; this
// cross-fade is the Reduce Motion presentation itself.
const fadeTiming = { ...reducedTiming, reduceMotion: ReduceMotion.Never };

interface SheetContext {
  readonly store: NewConversationStore;
  /** Closes the sheet (the xmark). */
  readonly close: () => void;
  /** True while the Reduce Motion sheet fades out: the toolbar is disabled. */
  readonly closing: boolean;
}

const Context = createContext<SheetContext | undefined>(undefined);

/** The sheet's store and close action; undefined while no session is open. */
export function useNewConversation(): SheetContext | undefined {
  return use(Context);
}

/**
 * Reduce Motion, known on the first render. The shared `useReducedMotion()`
 * starts false until its async read returns, which would show the faded
 * content opaque for a frame; this starts from Reanimated's launch value and
 * then follows the system setting.
 */
function useSheetReducedMotion(): boolean {
  const atLaunch = useLaunchReducedMotion();
  const [enabled, setEnabled] = useState(atLaunch);
  useEffect(() => {
    let current = true;
    void AccessibilityInfo.isReduceMotionEnabled().then(
      (value) => {
        if (current) setEnabled(value);
      },
      () => {},
    );
    const subscription = AccessibilityInfo.addEventListener(
      "reduceMotionChanged",
      (value) => {
        setEnabled(value);
      },
    );
    return () => {
      current = false;
      subscription.remove();
    };
  }, []);
  return enabled;
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
  const reduceMotion = useSheetReducedMotion();
  // `ReducedMotionSheet.present` commits the sheet transparent, then fades in.
  const opacity = useSharedValue(reduceMotion ? 0 : 1);
  const [closing, setClosing] = useState(false);
  const closingRef = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (!reduceMotion) return;
    opacity.value = withTiming(1, fadeTiming);
  }, [reduceMotion, opacity]);
  useEffect(() => () => clearTimeout(timer.current), []);

  const dismiss = useCallback(
    (then?: () => void) => {
      if (closingRef.current) return;
      closingRef.current = true;
      const leave = () => {
        // The layout is a screen of the root stack: this closes the sheet.
        navigation.goBack();
        then?.();
      };
      if (!reduceMotion) {
        leave();
        return;
      }
      // Like `ReducedMotionSheet`: block input, fade out, then dismiss
      // without animation.
      setClosing(true);
      opacity.value = withTiming(0, fadeTiming);
      timer.current = setTimeout(leave, reducedTiming.duration);
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
      closing,
    }),
    [store, dismiss, closing],
  );
  const fade = useAnimatedStyle(() => ({ opacity: opacity.value }));

  return (
    <Context value={value}>
      {!reduceMotion ? (
        children
      ) : (
        <Animated.View
          pointerEvents={closing ? "none" : "auto"}
          accessibilityElementsHidden={closing}
          importantForAccessibility={closing ? "no-hide-descendants" : "auto"}
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
