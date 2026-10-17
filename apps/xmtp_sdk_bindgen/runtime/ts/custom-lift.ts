import {
  ErrorCategory,
  type ErrorDetails,
  MessageBody,
  MessageBody_Tags,
  MessageContent,
  MessageContent_Tags,
  type EncodedContent,
} from "../xmtp_sdk";
import type { DecodedCustom } from "./custom-codec";

export type LiftedCustomBody = {
  tag: MessageBody_Tags.Custom;
  inner: {
    encoded: EncodedContent;
    rawBytes: ArrayBuffer;
    value?: unknown;
    error?: ErrorDetails;
  };
};

export type LiftedCustomContent = {
  tag: MessageContent_Tags.Custom;
  inner: {
    encoded: EncodedContent;
    rawBytes: ArrayBuffer;
    value?: unknown;
    error?: ErrorDetails;
  };
};

export function liftCustomBody(
  body: { inner: { encoded: EncodedContent; rawBytes: ArrayBuffer } },
  hasOwner: boolean,
  decoded: DecodedCustom | undefined,
): LiftedCustomBody | ReturnType<typeof MessageBody.Unknown.new> {
  const { encoded, rawBytes } = body.inner;
  if (decoded === undefined && hasOwner)
    return MessageBody.Unknown.new({
      encoded,
      rawBytes,
      error: codecNotFound(),
    });
  return {
    tag: MessageBody_Tags.Custom,
    inner: { encoded, rawBytes, ...(decoded ?? { error: clientClosed() }) },
  };
}

export function liftCustomContent(
  content: { inner: { encoded: EncodedContent; rawBytes: ArrayBuffer } },
  hasOwner: boolean,
  decoded: DecodedCustom | undefined,
): LiftedCustomContent | ReturnType<typeof MessageContent.Unknown.new> {
  const { encoded, rawBytes } = content.inner;
  if (decoded === undefined && hasOwner)
    return MessageContent.Unknown.new({
      encoded,
      rawBytes,
      error: codecNotFound(),
    });
  return {
    tag: MessageContent_Tags.Custom,
    inner: { encoded, rawBytes, ...(decoded ?? { error: clientClosed() }) },
  };
}

function codecNotFound(): ErrorDetails {
  return {
    code: "CodecNotFound",
    category: ErrorCategory.Input,
    retryable: false,
    message: "content type has no registered host codec",
    streamFailure: undefined,
  };
}

function clientClosed(): ErrorDetails {
  return {
    code: "ClientClosed",
    category: ErrorCategory.Lifecycle,
    retryable: false,
    message: "client is closed",
    streamFailure: undefined,
  };
}
