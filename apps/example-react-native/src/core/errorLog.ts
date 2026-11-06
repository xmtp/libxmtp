// Error text for console logs. Logs carry no full group, message or inbox ID.
// An SDK error's message can name one in more than one form ("group with id
// <hex> not found", Rust's debug lists of bytes), so an SDK error logs only
// its `details.code`, a fixed name such as `Storage`. Other error text keeps
// a short prefix of each ID it holds as hexadecimal or as a list of bytes.

/** Characters of a hexadecimal ID kept in a log line. */
export const LOGGED_ID_LENGTH = 8;

// 16 or more hexadecimal characters: group, message, inbox and installation
// IDs (64), legacy group IDs from imported archives (32) and addresses (40).
const HEX_ID = /[0-9a-f]{16,}/gi;
// Rust's debug form of 16 or more bytes: `[18, 52, 86, …]`.
const BYTE_LIST = /\[(?:\d{1,3}, ){15,}\d{1,3}\]/g;

/** The `details.code` of an SDK error (`XmtpError`). */
function sdkErrorCode(error: unknown): string | undefined {
  if (!(error instanceof Error) || !("details" in error)) return undefined;
  const { details } = error;
  return typeof details === "object" &&
    details !== null &&
    "code" in details &&
    typeof details.code === "string"
    ? details.code
    : undefined;
}

/**
 * Text that logs `error` without a full ID: `XmtpError.<code>` for an SDK
 * error, else the error's text with each ID cut to a short prefix.
 */
export function errorLogText(error: unknown): string {
  const code = sdkErrorCode(error);
  const text = code === undefined ? String(error) : `XmtpError.${code}`;
  return text
    .replace(HEX_ID, (id) => `${id.slice(0, LOGGED_ID_LENGTH)}…`)
    .replace(BYTE_LIST, "[…]");
}
