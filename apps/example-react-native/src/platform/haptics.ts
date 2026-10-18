import * as Haptics from "expo-haptics";

// Haptic feedback is best effort: a device without a Taptic Engine, or a
// disabled setting, must never fail the action that asked for it.

function quietly(feedback: Promise<void>): void {
  feedback.catch(() => undefined);
}

/** A light tap (for example, a selection or a toggle). */
export function selectionFeedback(): void {
  quietly(Haptics.selectionAsync());
}

/** An impact (for example, a long-press menu or a reaction). */
export function impactFeedback(
  style: "light" | "medium" | "heavy" | "soft" | "rigid" = "medium",
): void {
  const styles = {
    light: Haptics.ImpactFeedbackStyle.Light,
    medium: Haptics.ImpactFeedbackStyle.Medium,
    heavy: Haptics.ImpactFeedbackStyle.Heavy,
    soft: Haptics.ImpactFeedbackStyle.Soft,
    rigid: Haptics.ImpactFeedbackStyle.Rigid,
  } as const;
  quietly(Haptics.impactAsync(styles[style]));
}

/** A success, warning or error notification. */
export function notificationFeedback(
  type: "success" | "warning" | "error",
): void {
  const types = {
    success: Haptics.NotificationFeedbackType.Success,
    warning: Haptics.NotificationFeedbackType.Warning,
    error: Haptics.NotificationFeedbackType.Error,
  } as const;
  quietly(Haptics.notificationAsync(types[type]));
}
