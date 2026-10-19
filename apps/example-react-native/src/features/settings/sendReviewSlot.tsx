import type { ActiveSession } from "@/session";

export interface SendReviewSectionProps {
  readonly session: ActiveSession;
}

/**
 * The "Send needs review" section of Settings.
 * Until the messaging screens exist, this placeholder renders nothing.
 */
export function SendReviewSection(_props: SendReviewSectionProps) {
  return null;
}
