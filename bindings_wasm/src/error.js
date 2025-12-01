/**
 * Custom error class for XMTP WASM bindings.
 * Extends the standard Error class to provide structured error information.
 */
export class XmtpError extends Error {
  /**
   * @param {string} code - Error code category (e.g., "CONVERSATION_ERROR")
   * @param {string} message - Human-readable error message
   * @param {string|null} kind - Specific error kind/variant (e.g., "NotFound", "UserLimitExceeded")
   * @param {object|null} details - Optional structured details about the error
   */
  constructor(code, message, kind, details) {
    super(message);
    this.name = "XmtpError";
    this.code = code;
    this.kind = kind || null;
    this.details = details || null;
  }
}
