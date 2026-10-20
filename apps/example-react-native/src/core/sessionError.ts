// Session errors and their copy.

export const SessionCopy = {
  stale: "This session has ended.",
  missingSecret: "Cannot restore this account. Try again.",
  finishReset: "Finish local reset before connecting",
  invalidUrl: "Use a valid HTTP or HTTPS backend URL",
  urlParts:
    "Use a backend URL without credentials, a query, a fragment, or an invalid port",
  remoteHttp: "Use HTTPS for a remote backend",
  enterToken: "Enter an access token.",
  tokenRejected: "Access token was not accepted. Check it and try again.",
  unreachable: "Cannot reach this server. Try again.",
  unsafePath: "Unsafe profile path",
  signOutFailed: "Cannot sign out. Try again.",
  resetIncomplete: "Local deletion is incomplete",
  appearanceFailed: "Cannot save appearance. Try again.",
  storageTitle: "Cannot open app",
  storageMessage: "Cannot open local account storage.",
} as const;

export type SessionErrorKind = "message" | "stale" | "missingSecret";

/** An error whose `message` is user-facing copy. */
export class SessionError extends Error {
  readonly kind: SessionErrorKind;

  private constructor(kind: SessionErrorKind, message: string) {
    super(message);
    this.name = "SessionError";
    this.kind = kind;
  }

  /** An error that shows `text`. */
  static message(text: string): SessionError {
    return new SessionError("message", text);
  }

  /** The session that started the work has ended or been replaced. */
  static stale(): SessionError {
    return new SessionError("stale", SessionCopy.stale);
  }

  /** A saved secret is missing or malformed; restore must not replace it. */
  static missingSecret(): SessionError {
    return new SessionError("missingSecret", SessionCopy.missingSecret);
  }
}

/** Work that its caller cancelled (an aborted signal). */
export class CancelledError extends Error {
  constructor() {
    super("The operation was cancelled.");
    this.name = "AbortError";
  }
}

/** True for a `CancelledError` or a DOM-style `AbortError`. */
export function isCancelled(error: unknown): boolean {
  return error instanceof Error && error.name === "AbortError";
}

/** The user-facing text of an error. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
