import type { ContentTypeId, EncodedContent } from "../../public-values.gen";

/**
 * A content codec over public values. The send helpers run its steps before
 * the send starts: `encode`, then `fallback` when the envelope has none, then
 * `shouldPush` when the send has no explicit `shouldPush` option. A step that
 * throws, rejects, or returns a value of the wrong type fails the send with
 * `XmtpError.CodecEncodeFailed`, and the SDK makes no publish attempt. Each
 * step is a function property, so the value type is checked in both
 * directions.
 */
export interface ContentCodec<T> {
  readonly type: ContentTypeId;
  readonly encode: (value: T) => EncodedContent;
  readonly decode: (encoded: EncodedContent) => T;
  /** Text for recipients without this codec. Absent: no fallback. */
  readonly fallback?: (value: T) => string | undefined;
  /** Whether sending this value notifies recipients. Absent: it does. */
  readonly shouldPush?: (value: T) => boolean;
}

/**
 * A codec of any value type, as a client registers it. Registration only
 * decodes received content, so the value type is erased here.
 */
export type AnyContentCodec = {
  readonly type: ContentTypeId;
  encode(value: never): EncodedContent;
  decode(encoded: EncodedContent): unknown;
};

// Both browser trees use this private marker. It stores the exact builtin
// fallback method on the codec, so a subclass override keeps its hook.
const rustStandardFallback = Symbol.for("@xmtp/sdk/rust-standard-fallback");
export function registerRustStandardFallback(
  codec: object,
  fallback: unknown,
): void {
  Object.defineProperty(codec, rustStandardFallback, { value: fallback });
}
export function usesRustStandardFallback(
  codec: object,
  fallback: unknown,
): boolean {
  return (
    Reflect.get(codec, rustStandardFallback) === fallback &&
    Reflect.has(codec, rustStandardFallback)
  );
}
