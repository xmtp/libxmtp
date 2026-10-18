import { useMemo } from "react";
import {
  Easing,
  type EntryExitAnimationFunction,
  withSpring,
  withTiming,
} from "react-native-reanimated";

import { useReducedMotion } from "@/ui/accessibility";

/**
 * `MotionPolicy` (`DesignSystem/Motion.swift`). SwiftUI
 * `.spring(response: 0.28, dampingFraction: 0.86)` with mass 1 is stiffness
 * (2π/0.28)² and damping 4π·0.86/0.28.
 */
export const springConfig = {
  stiffness: 503.6,
  damping: 38.6,
  mass: 1,
} as const;

/** Reduce Motion: `.easeInOut(duration: 0.2)`. */
export const reducedTiming = {
  duration: 200,
  easing: Easing.bezier(0.42, 0, 0.58, 1),
} as const;

/** `MotionPolicy.transition` scales from 0.96 while it fades in. */
export const transitionScale = 0.96;

export interface Motion {
  reduceMotion: boolean;
  /** `motion.animation` toward `value`. Usable inside worklets. */
  animate: (value: number) => number;
  /** `motion.transition` for Reanimated `entering`. */
  entering: EntryExitAnimationFunction;
  /** `motion.transition` for Reanimated `exiting`. */
  exiting: EntryExitAnimationFunction;
}

function springTo(value: number): number {
  "worklet";
  return withSpring(value, springConfig);
}

function timeTo(value: number): number {
  "worklet";
  return withTiming(value, reducedTiming);
}

const fullEntering: EntryExitAnimationFunction = () => {
  "worklet";
  return {
    initialValues: { opacity: 0, transform: [{ scale: transitionScale }] },
    animations: {
      opacity: withSpring(1, springConfig),
      transform: [{ scale: withSpring(1, springConfig) }],
    },
  };
};

const fullExiting: EntryExitAnimationFunction = () => {
  "worklet";
  return {
    initialValues: { opacity: 1, transform: [{ scale: 1 }] },
    animations: {
      opacity: withSpring(0, springConfig),
      transform: [{ scale: withSpring(transitionScale, springConfig) }],
    },
  };
};

const reducedEntering: EntryExitAnimationFunction = () => {
  "worklet";
  return {
    initialValues: { opacity: 0 },
    animations: { opacity: withTiming(1, reducedTiming) },
  };
};

const reducedExiting: EntryExitAnimationFunction = () => {
  "worklet";
  return {
    initialValues: { opacity: 1 },
    animations: { opacity: withTiming(0, reducedTiming) },
  };
};

/** The motion policy for the current Reduce Motion setting. */
export function useMotion(): Motion {
  const reduceMotion = useReducedMotion();
  return useMemo(
    () =>
      reduceMotion
        ? {
            reduceMotion,
            animate: timeTo,
            entering: reducedEntering,
            exiting: reducedExiting,
          }
        : {
            reduceMotion,
            animate: springTo,
            entering: fullEntering,
            exiting: fullExiting,
          },
    [reduceMotion],
  );
}
