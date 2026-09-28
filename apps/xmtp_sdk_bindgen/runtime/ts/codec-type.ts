import type { ContentTypeID, EncodedContent } from "../xmtp_sdk";

export interface ContentCodec<T> {
  readonly type: ContentTypeID;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
}
