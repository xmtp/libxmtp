// Links to the Debug session probe. Any app can open the app's URL scheme, so
// a link cannot sign out or delete local data by itself: the probe asks in
// the app first. Other actions stay automatable for UI checks.

/** A session probe action that a link runs only after an in-app confirmation. */
export type ProbeLinkConfirmation = "signOut" | "delete";

/** The confirmation a link to the session probe needs before `action` runs. */
export function probeLinkConfirmation(
  action: string,
): ProbeLinkConfirmation | undefined {
  return action === "signOut" || action === "delete" ? action : undefined;
}
