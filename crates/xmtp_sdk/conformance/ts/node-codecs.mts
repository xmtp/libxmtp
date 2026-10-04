import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

export function assertEncodedEqual(
  actual: sdk.EncodedContent,
  expected: sdk.EncodedContent,
): void {
  assert.deepEqual(actual.type, expected.type);
  assert.deepEqual(actual.parameters, expected.parameters);
  assert.equal(actual.fallback, expected.fallback);
  assert.ok(actual.content instanceof Uint8Array);
  assert.deepEqual(Buffer.from(actual.content), Buffer.from(expected.content));
}

export function isInvalidId(error: unknown): boolean {
  if (!(error instanceof sdk.XmtpError.InvalidArgument)) return false;
  assert.equal(error.details.code, "InvalidArgument");
  assert.equal(error.details.category, "input");
  assert.equal(error.details.retryable, false);
  return true;
}

type Kind = sdk.StandardContent["kind"];

export function checkStandardCodecs() {
  const standardCodecs = new Map<Kind, sdk.AnyContentCodec>([
    ["text", new sdk.TextCodec()],
    ["markdown", new sdk.MarkdownCodec()],
    ["readReceipt", new sdk.ReadReceiptCodec()],
    ["reaction", new sdk.ReactionV2Codec()],
    ["attachment", new sdk.AttachmentCodec()],
    ["remoteAttachment", new sdk.RemoteAttachmentCodec()],
    ["multiRemoteAttachment", new sdk.MultiRemoteAttachmentCodec()],
    ["transactionReference", new sdk.TransactionReferenceCodec()],
    ["walletSendCalls", new sdk.WalletSendCallsCodec()],
    ["actions", new sdk.ActionsCodec()],
    ["intent", new sdk.IntentCodec()],
    ["reply", new sdk.ReplyCodec()],
    ["groupUpdated", new sdk.GroupUpdatedCodec()],
    ["deleteMessage", new sdk.DeleteMessageCodec()],
    ["leaveRequest", new sdk.LeaveRequestCodec()],
  ]);
  const codecSamples = sdk.sdkConformanceStandardSamples();
  assert.equal(codecSamples.length, 15);
  for (const sample of codecSamples) {
    const content = sample.value;
    const codec = standardCodecs.get(content.kind);
    assert.ok(codec, `missing codec for ${content.kind}`);
    // Whole-content codecs take the variant; the others take its value.
    const value =
      content.kind === "readReceipt"
        ? undefined
        : content.kind === "reaction" ||
            content.kind === "reply" ||
            content.kind === "deleteMessage"
          ? content
          : content.value;
    const encoded = codec.encode(value as never);
    assert.equal(
      codec.fallback?.(value as never),
      sample.expected.fallback,
      `canonical fallback for ${content.kind}`,
    );
    assert.equal(
      codec.shouldPush?.(value as never),
      sdk.catalogueContentTypeShouldPush(codec.type),
      `catalogue push default for ${content.kind}`,
    );
    if (content.kind === "leaveRequest") {
      assert.equal(codec.shouldPush?.(value as never), false);
      assert.equal(
        codec.fallback?.(value as never),
        "A member has requested leaving the group",
      );
    }
    assertEncodedEqual(encoded, sample.expected);
    assertEncodedEqual(
      codec.encode(codec.decode(encoded) as never),
      sample.expected,
    );
  }
  console.log("Node P69: all 15 standard codecs match Rust bytes");

  const malformedDelete: sdk.StandardContent = {
    kind: "deleteMessage",
    messageId: "bad",
  };
  assert.throws(() => sdk.encodeStandard(malformedDelete), isInvalidId);
  assert.throws(
    () => new sdk.DeleteMessageCodec().encode(malformedDelete),
    isInvalidId,
  );

  return codecSamples;
}
