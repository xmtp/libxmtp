import type { ContentTypeId, EncodedContent } from "../../public-values.gen";

/** A content codec over public values. */
export interface ContentCodec<T> {
  readonly type: ContentTypeId;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
}
