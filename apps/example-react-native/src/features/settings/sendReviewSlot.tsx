import type { ActiveSession } from "@/session";

export interface SendReviewSectionProps {
  readonly session: ActiveSession;
}

/**
 * The "Send needs review" section of Settings (`SendReviewSection.swift`).
 * The messaging lane replaces this placeholder, which renders nothing.
 */
export function SendReviewSection(_props: SendReviewSectionProps) {
  return null;
}
