import type { ContentTypeId, EncodedContent } from "../xmtp_sdk";

export interface ContentCodec<T> {
  readonly type: ContentTypeId;
  encode(value: T): EncodedContent;
  decode(encoded: EncodedContent): T;
}
