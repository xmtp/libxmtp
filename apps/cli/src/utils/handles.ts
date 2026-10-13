import { XmtpError } from "@xmtp/node-sdk";

import { escapeControlCharacters } from "./output.js";

/**
 * The handle grammar (IDENT section 10): a name part, then an optional code.
 * It is the one rule a client may check; the backend applies the minimum
 * length and the reserved names.
 */
const HANDLE_PATTERN = /^[a-z][a-z0-9_-]{0,26}(#[0-9]{4})?$/;

/** Whether input, after the SDK's ASCII lowercasing, matches the grammar. */
export function isValidHandle(input: string): boolean {
  return HANDLE_PATTERN.test(asciiLowercase(input));
}

/** Lowercase ASCII letters only, as the SDK does with handle input. */
export function asciiLowercase(value: string): string {
  return value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

/** The name part of a handle: the text before `#`. */
export function handleName(handle: string): string {
  return handle.split("#", 1)[0];
}

/**
 * The handle fields of an inbox state or a member for JSON output. The
 * current handle is `null` when the inbox has none, or when there is no
 * inbox.
 */
export function handleFields(
  record:
    | {
        readonly handle?: string | undefined;
        readonly otherHandles: readonly string[];
      }
    | undefined,
): { handle: string | null; otherHandles: string[] } {
  return {
    handle: record?.handle ?? null,
    otherHandles: [...(record?.otherHandles ?? [])],
  };
}

/** The other handles of an inbox as one line of text. */
export function formatOtherHandles(otherHandles: readonly string[]): string {
  return otherHandles.join(", ");
}

/** The message for input that the handle grammar does not match. */
export function invalidHandleMessage(input: string): string {
  return `${escapeControlCharacters(input)} is not a valid handle. Start with a letter, then use only lowercase letters, numbers, _ and -, 27 characters or fewer before #. After #, use exactly 4 digits, like nick#0012.`;
}

/** The message for handles, in canonical text, that no inbox holds. */
export function handleNotFoundMessage(handles: readonly string[]): string {
  return handles.length === 1
    ? `No inbox has the handle ${handles[0]}.`
    : `No inbox has the handles ${handles.join(", ")}.`;
}

/**
 * The message for a failed claim of `input`, or `undefined` when the error
 * is not a handle failure. `ownOtherHandles` are the other handles of the
 * claiming inbox, read before the claim: a claim of one of them is a switch
 * back.
 */
export function claimFailureMessage(
  error: unknown,
  input: string,
  ownOtherHandles: readonly string[],
): string | undefined {
  const handle = asciiLowercase(input);
  if (error instanceof XmtpError.HandleTaken) {
    return `${handle} is taken. Pick another handle. Run 'xmtp client handle-available ${handleName(handle)}' to see free handles.`;
  }
  if (error instanceof XmtpError.HandleNotAllowed) {
    return ownOtherHandles.includes(handle)
      ? `${handle} is not allowed on this server now. It still finds you, but you cannot switch back to it.`
      : `${handle} is not allowed on this server. Choose a different name.`;
  }
  if (error instanceof XmtpError.HandleTooSoon) {
    return "You changed your handle less than 7 days ago. You can change your handle once every 7 days.";
  }
  if (error instanceof XmtpError.HandleLimitReached) {
    return "This inbox has used all 64 handle changes. Your current handle stays, and so do your other handles.";
  }
  if (error instanceof XmtpError.InvalidArgument) {
    return invalidHandleMessage(input);
  }
  return undefined;
}
