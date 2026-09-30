// The host send policy for typed codecs (Ref Public surface, Host codecs).
// Every codec step runs before the send starts, so a failed step makes no
// publish attempt.
import { XmtpError, type EncodedContent } from "../../public-values.gen";
import type { ContentCodec } from "./codec";

function codecFailed(step: string, cause: unknown): XmtpError {
  const reason = cause instanceof Error ? cause.message : String(cause);
  return new XmtpError.CodecEncodeFailed({
    code: "CodecEncodeFailed",
    category: "callback",
    retryable: false,
    message: `content codec ${step} failed: ${reason}`,
  });
}

// A codec step is synchronous. A Promise is not a valid result.
function isThenable(value: unknown): boolean {
  return (
    value !== null &&
    (typeof value === "object" || typeof value === "function") &&
    typeof Reflect.get(value, "then") === "function"
  );
}

function isEncodedContent(value: unknown): value is EncodedContent {
  return (
    value !== null &&
    typeof value === "object" &&
    !isThenable(value) &&
    "type" in value &&
    "content" in value &&
    value.content instanceof Uint8Array
  );
}

/**
 * The envelope of `value` for a send. An envelope that already has a fallback
 * keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
 * it. A failed or invalid step is `CodecEncodeFailed`.
 */
export function encodeForSend<T>(
  codec: ContentCodec<T>,
  value: T,
): EncodedContent {
  let encoded: unknown;
  try {
    encoded = codec.encode(value);
  } catch (error) {
    throw codecFailed("encode", error);
  }
  if (!isEncodedContent(encoded))
    throw codecFailed("encode", "the result is not EncodedContent");
  if (encoded.fallback !== undefined || codec.fallback === undefined)
    return encoded;
  let fallback: unknown;
  try {
    fallback = codec.fallback(value);
  } catch (error) {
    throw codecFailed("fallback", error);
  }
  if (fallback === undefined) return encoded;
  if (typeof fallback !== "string")
    throw codecFailed("fallback", "the result is not a string");
  return { ...encoded, fallback };
}
