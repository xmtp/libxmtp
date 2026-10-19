// Links to the Debug session and chat probes. Any app can open the app's URL
// scheme, so a session probe link asks in the app before an action that ends
// or replaces the session, deletes local data, or changes what other people
// see:
//
// - `signOut` and `delete`.
// - `allowRequests` (accepts every message request), `claim` (the claim
//   stays in the identity log) and `createGroup` (adds the members the link
//   names).
// - `connect` and `retry`, in every session state. With a session open they
//   end or replace it. Signed out, `connect` still opens a session: it
//   reopens the identity this device keeps for the server the link names, or
//   creates one there, so a link must not choose the server. `retry` reopens
//   the saved profile whose restore failed, on its own server. The signed-out
//   confirmations (`join`, `reopen`) say so instead of naming a session to
//   end. Like Join with an empty Access token, a connect link without a token
//   clears the token saved for that server.
//
// The other actions run at once: `start` and `restore` do what launch does,
// `probe` and `handles` only read, `continueReset` only finishes a deletion
// the user confirmed, and `contend` holds the message reader's lease for
// 15 s. The probe's own buttons always run at once.
//
// The chat probe has no buttons. A link to it asks before an action that
// changes data: `retry`, `react` and `delete` change what other people see,
// and `reply` and `edit` replace the chat's saved draft (its reply target or
// its text). The send faults `failNextSend`, `failPublish` and
// `allowPublish` run at once: they change nothing until the user sends, last
// only until the app restarts, and UI checks set them by link.

import type { SessionState } from "./sessionController.ts";

/**
 * A confirmation a session probe link needs before its action runs. `join`
 * and `reopen` are `connect` and `retry` while the app is signed out.
 */
export type ProbeLinkConfirmation =
  | "signOut"
  | "delete"
  | "connect"
  | "join"
  | "retry"
  | "reopen"
  | "allowRequests"
  | "claim"
  | "createGroup";

/**
 * The confirmation a link to the session probe needs before `action` runs,
 * in the session state the link finds. Undefined: the link runs it at once.
 */
export function probeLinkConfirmation(
  action: string,
  session: SessionState<unknown>["kind"],
): ProbeLinkConfirmation | undefined {
  switch (action) {
    case "signOut":
    case "delete":
    case "allowRequests":
    case "claim":
    case "createGroup":
      return action;
    case "connect":
      return session === "signedOut" ? "join" : "connect";
    case "retry":
      return session === "signedOut" ? "reopen" : "retry";
    default:
      return undefined;
  }
}

/** A chat probe action that a link runs only after an in-app confirmation. */
export type ChatProbeLinkConfirmation =
  | "retry"
  | "react"
  | "delete"
  | "reply"
  | "edit";

/**
 * The confirmation a link to the chat probe needs before `action` runs.
 * Undefined: the link runs it at once.
 */
export function chatProbeLinkConfirmation(
  action: string,
): ChatProbeLinkConfirmation | undefined {
  switch (action) {
    case "retry":
    case "react":
    case "delete":
    case "reply":
    case "edit":
      return action;
    default:
      return undefined;
  }
}
