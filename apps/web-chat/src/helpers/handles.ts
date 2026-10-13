import { XmtpError } from "@xmtp/browser-sdk";

/** The handle grammar (IDENT section 10): a name part, then an optional code. */
export const HANDLE_PATTERN = /^[a-z][a-z0-9_-]{0,26}(#[0-9]{4})?$/;

/** The longest name part that the grammar allows. */
export const MAX_HANDLE_NAME_LENGTH = 27;

/** How many suggestions the editor shows at first, and adds per "Show more". */
export const HANDLE_SUGGESTION_PAGE = 3;

export const isValidHandle = (value: string) => HANDLE_PATTERN.test(value);

/** The name part and the optional code of a handle or of handle input. */
export const splitHandle = (value: string) => {
  const hash = value.indexOf("#");
  return hash === -1
    ? { name: value, code: undefined }
    : { name: value.slice(0, hash), code: value.slice(hash + 1) };
};

/** The supporting text under a handle field. */
export const handleHint = (minNameLength: number) =>
  `${minNameLength} to ${MAX_HANDLE_NAME_LENGTH} lowercase letters, numbers, _ or -.`;

/**
 * Checks handle input as the user types: the grammar, then the published
 * minimum name length. This check is app-level only (D30): the SDK
 * never applies the minimum, and the backend decides when the user claims.
 * Returns the message to show, or null when the input can be checked.
 */
export const checkHandleInput = (
  input: string,
  minNameLength: number,
): string | null => {
  if (input === "") return null;
  const { name, code } = splitHandle(input);
  if (/[^a-z0-9_-]/.test(name)) {
    return "Use only lowercase letters, numbers, _ and -.";
  }
  if (!/^[a-z]/.test(name)) return "Start with a letter.";
  if (name.length > MAX_HANDLE_NAME_LENGTH) {
    return `Use ${MAX_HANDLE_NAME_LENGTH} characters or fewer before #.`;
  }
  if (code !== undefined && !/^[0-9]{4}$/.test(code)) {
    return `After #, use exactly 4 digits, like ${name}#0012.`;
  }
  if (name.length < minNameLength) {
    return `Use at least ${minNameLength} characters on this server.`;
  }
  return null;
};

/** The backend's answer to a claim, as the app shows it. */
export type ClaimOutcome =
  | "claimed"
  | "taken"
  | "notAllowed"
  | "tooSoon"
  | "limitReached"
  | "failed";

/** Maps a `claimHandle` failure to the answer that the app shows. */
export const claimFailure = (
  error: unknown,
): Exclude<ClaimOutcome, "claimed"> => {
  if (error instanceof XmtpError.HandleTaken) return "taken";
  if (error instanceof XmtpError.HandleNotAllowed) return "notAllowed";
  if (error instanceof XmtpError.HandleTooSoon) return "tooSoon";
  if (error instanceof XmtpError.HandleLimitReached) return "limitReached";
  return "failed";
};
