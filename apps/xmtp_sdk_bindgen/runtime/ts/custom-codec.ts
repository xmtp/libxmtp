import type { ContentTypeID, EncodedContent } from "../xmtp_sdk";

export type AnyCodec = {
  readonly type: ContentTypeID;
  encode(value: never): EncodedContent;
  decode(encoded: EncodedContent): unknown;
};

export type DecodedCustom = { value?: unknown; error?: string };

export function codecKey(type: ContentTypeID): string {
  return JSON.stringify([type.authorityID, type.typeID, type.versionMajor]);
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
    return { error: String(error) };
  }
}
