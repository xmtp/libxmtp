import type { ContentCodec, ContentTypeId, EncodedContent } from "xmtp-sdk";

export type Reading = { readonly text: string; readonly revision: number };

// A codec package needs only the new SDK's public contract.
export class ReadingCodec implements ContentCodec<Reading> {
  readonly type: ContentTypeId;
  fallbackCalls = 0;
  pushCalls = 0;

  constructor(
    minor = 0,
    readonly envelopeFallback?: string,
  ) {
    this.type = {
      authorityId: "example.org",
      typeId: "reading",
      versionMajor: 1,
      versionMinor: minor,
    };
  }

  encode = (value: Reading): EncodedContent => ({
    type: this.type,
    parameters: new Map([
      ["format", "revision-newline-text"],
      ["label", "temperature °C"],
    ]),
    fallback: this.envelopeFallback,
    content: new TextEncoder().encode(`${value.revision}\n${value.text}`),
  });

  decode = (encoded: EncodedContent): Reading => {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(
      encoded.content,
    );
    const split = text.indexOf("\n");
    const revision = Number(text.slice(0, split));
    if (split < 1 || !Number.isSafeInteger(revision))
      throw new Error("invalid reading revision");
    return { text: text.slice(split + 1), revision };
  };

  fallback = (value: Reading): string => {
    this.fallbackCalls += 1;
    return `reading ${value.revision}: ${value.text}`;
  };

  shouldPush = (_value: Reading): boolean => {
    this.pushCalls += 1;
    return false;
  };
}
