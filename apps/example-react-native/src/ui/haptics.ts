import * as Haptics from "expo-haptics";

function ignore(): void {}

/**
 * Haptic feedback. Fire and forget: failures (no haptic engine, simulator)
 * are ignored.
 */
export const haptics = {
  /** `UIImpactFeedbackGenerator(style: .light)` (swipe to reply arms). */
  lightImpact(): void {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light).catch(ignore);
  },
  /** `UISelectionFeedbackGenerator`. */
  selection(): void {
    Haptics.selectionAsync().catch(ignore);
  },
  success(): void {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(
      ignore,
    );
  },
  error(): void {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error).catch(
      ignore,
    );
  },
} as const;
