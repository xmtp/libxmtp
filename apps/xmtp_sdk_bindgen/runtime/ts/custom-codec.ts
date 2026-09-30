import {
  ErrorCategory,
  type ErrorDetails,
  type ContentTypeId,
  type EncodedContent,
} from "../xmtp_sdk";

export type AnyCodec = {
  readonly type: ContentTypeId;
  encode(value: never): EncodedContent;
  decode(encoded: EncodedContent): unknown;
};

export type DecodedCustom = { value?: unknown; error?: ErrorDetails };

export function codecKey(type: ContentTypeId): string {
  return JSON.stringify([type.authorityId, type.typeId, type.versionMajor]);
}

export function decodeCustom(
  codecs: ReadonlyMap<string, AnyCodec> | undefined,
  encoded: EncodedContent,
): DecodedCustom | undefined {
  const codec = codecs?.get(codecKey(encoded.type));
  if (codec === undefined) return undefined;
  try {
    return { value: codec.decode(encoded) };
  } catch (error) {
    let message: string;
    try {
      message = String(error);
    } catch {
      message = "custom content codec failed";
    }
    return {
      error: {
        code: "CodecDecodeFailed",
        category: ErrorCategory.Callback,
        retryable: false,
        message,
      },
    };
  }
}
