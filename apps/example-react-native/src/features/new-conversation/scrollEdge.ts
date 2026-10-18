import type { NativeStackNavigationOptions } from "expo-router";
import { useHeaderHeight } from "expo-router/react-navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NativeScrollEvent, NativeSyntheticEvent } from "react-native";

/** Content counts as under the bar once it moves past this many points. */
const SCROLLED_THRESHOLD = 0.5;

interface ScrollEdge {
  /** Stack screen options with the top edge effect for the scroll position. */
  readonly options: Pick<NativeStackNavigationOptions, "scrollEdgeEffects">;
  /** The form's scroll handler (use with `scrollEventThrottle={16}`). */
  readonly onScroll: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
}

/**
 * A SwiftUI Form in a sheet shows the hard edge (a band and a hairline) under
 * its bar only while content is under the bar. UIKit draws a hard edge effect
 * even at rest, so the screen asks for it only once scrolled. At rest the
 * content offset is minus the bar height (automatic content insets).
 */
export function useScrollEdge(): ScrollEdge {
  const headerHeight = useHeaderHeight();
  const height = useRef(headerHeight);
  const offset = useRef<number | undefined>(undefined);
  const [scrolled, setScrolled] = useState(false);

  // The bar's height settles after the first layout: re-check the offset.
  useEffect(() => {
    height.current = headerHeight;
    if (offset.current === undefined) return;
    setScrolled(offset.current + headerHeight > SCROLLED_THRESHOLD);
  }, [headerHeight]);

  const onScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      offset.current = event.nativeEvent.contentOffset.y;
      // React skips the render while the value stays the same.
      setScrolled(offset.current + height.current > SCROLLED_THRESHOLD);
    },
    [],
  );
  const options = useMemo(
    (): ScrollEdge["options"] => ({
      scrollEdgeEffects: { top: scrolled ? "hard" : "automatic" },
    }),
    [scrolled],
  );
  return { options, onScroll };
}
