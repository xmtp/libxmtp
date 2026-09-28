import {
  MessageBody,
  MessageBody_Tags,
  MessageContent,
  MessageContent_Tags,
  type EncodedContent,
} from "../xmtp_sdk";
import type { DecodedCustom } from "./custom-codec";

export type LiftedCustomBody = {
  tag: MessageBody_Tags.Custom;
  inner: { encoded: EncodedContent; value?: unknown; error?: string };
};

export type LiftedCustomContent = {
  tag: MessageContent_Tags.Custom;
  inner: {
    encoded: EncodedContent;
    rawBytes: ArrayBuffer;
    value?: unknown;
    error?: string;
  };
};

export function liftCustomBody(
  body: { inner: { encoded: EncodedContent } },
  hasOwner: boolean,
  decoded: DecodedCustom | undefined,
): LiftedCustomBody | ReturnType<typeof MessageBody.Unknown.new> {
  const encoded = body.inner.encoded;
  if (decoded === undefined && hasOwner)
    return MessageBody.Unknown.new({ encoded });
  return {
    tag: MessageBody_Tags.Custom,
    inner: { encoded, ...(decoded ?? { error: "clientClosed" }) },
  };
}

export function liftCustomContent(
  content: { inner: { encoded: EncodedContent; rawBytes: ArrayBuffer } },
  hasOwner: boolean,
  decoded: DecodedCustom | undefined,
): LiftedCustomContent | ReturnType<typeof MessageContent.Unknown.new> {
  const { encoded, rawBytes } = content.inner;
  if (decoded === undefined && hasOwner)
    return MessageContent.Unknown.new({ encoded, rawBytes });
  return {
    tag: MessageContent_Tags.Custom,
    inner: { encoded, rawBytes, ...(decoded ?? { error: "clientClosed" }) },
  };
}
