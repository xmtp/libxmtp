import type { ContentTypeId, EncodedContent } from "../../public-values.gen";

/**
 * A content codec over public values. The send helpers run its steps before
 * the send starts: `encode`, then `fallback` when the envelope has none, then
 * `shouldPush` when the send has no explicit `shouldPush` option. A step that
 * throws, or returns a value of the wrong type, fails the send with
 * `XmtpError.CodecEncodeFailed`, and the SDK makes no publish attempt.
 */
export interface ContentCodec<T> {
  readonly type: ContentTypeId;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
  /** Text for recipients without this codec. Absent: no fallback. */
  fallback?(value: T): string | undefined;
  /** Whether sending this value notifies recipients. Absent: it does. */
  shouldPush?(value: T): boolean;
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
