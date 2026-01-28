/**
 * XMTP error with parsed error code.
 *
 * Error codes follow the format "TypeName::VariantName", e.g.:
 * - "GroupError::NotFound"
 * - "GroupError::PermissionDenied"
 * - "ClientError::NotRegistered"
 *
 * @example
 * ```typescript
 * try {
 *   await client.conversations.getGroup(groupId);
 * } catch (e) {
 *   if (e instanceof XmtpError) {
 *     console.log(`XMTP Error [${e.code}]: ${e.message}`);
 *   }
 * }
 * ```
 */
export class XmtpError extends Error {
  /** The structured error code, e.g. "GroupError::NotFound" */
  readonly code: string

  constructor(message: string, code: string, originalStack?: string) {
    super(message)
    this.name = 'XmtpError'
    this.code = code
    if (originalStack) {
      this.stack = originalStack
    }
    // Ensure proper prototype chain for instanceof checks
    Object.setPrototypeOf(this, XmtpError.prototype)
  }
}

export function parseXmtpError(error: unknown): XmtpError {
  if (!(error instanceof Error)) {
    const msg = String(error)
    return new XmtpError(msg, 'Unknown')
  }

  const match = error.message.match(/^\[([^\]]+)\]/)
  if (match) {
    const [, code] = match
    return new XmtpError(error.message, code, error.stack)
  }

  return new XmtpError(error.message, 'Unknown', error.stack)
}
