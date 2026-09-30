import type { ContentTypeId, EncodedContent } from "../../public-values.gen";

/** A content codec over public values. */
export interface ContentCodec<T> {
  readonly type: ContentTypeId;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
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
