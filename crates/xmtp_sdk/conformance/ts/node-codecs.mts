import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

export function assertEncodedEqual(
  actual: sdk.EncodedContent,
  expected: sdk.EncodedContent,
): void {
  assert.deepEqual(actual.type, expected.type);
  assert.deepEqual(actual.parameters, expected.parameters);
  assert.equal(actual.fallback, expected.fallback);
  assert.deepEqual(Buffer.from(actual.content), Buffer.from(expected.content));
}

export function checkStandardCodecs() {
  const standardCodecs = new Map([
    [sdk.StandardContent_Tags.Text, new sdk.TextCodec()],
    [sdk.StandardContent_Tags.Markdown, new sdk.MarkdownCodec()],
    [sdk.StandardContent_Tags.ReadReceipt, new sdk.ReadReceiptCodec()],
    [sdk.StandardContent_Tags.Reaction, new sdk.ReactionV2Codec()],
    [sdk.StandardContent_Tags.Attachment, new sdk.AttachmentCodec()],
    [
      sdk.StandardContent_Tags.RemoteAttachment,
      new sdk.RemoteAttachmentCodec(),
    ],
    [
      sdk.StandardContent_Tags.MultiRemoteAttachment,
      new sdk.MultiRemoteAttachmentCodec(),
    ],
    [
      sdk.StandardContent_Tags.TransactionReference,
      new sdk.TransactionReferenceCodec(),
    ],
    [sdk.StandardContent_Tags.WalletSendCalls, new sdk.WalletSendCallsCodec()],
    [sdk.StandardContent_Tags.Actions, new sdk.ActionsCodec()],
    [sdk.StandardContent_Tags.Intent, new sdk.IntentCodec()],
    [sdk.StandardContent_Tags.Reply, new sdk.ReplyCodec()],
    [sdk.StandardContent_Tags.GroupUpdated, new sdk.GroupUpdatedCodec()],
    [sdk.StandardContent_Tags.DeleteMessage, new sdk.DeleteMessageCodec()],
    [sdk.StandardContent_Tags.LeaveRequest, new sdk.LeaveRequestCodec()],
  ]);
  const codecSamples = sdk.sdkConformanceStandardSamples();
  assert.equal(codecSamples.length, 15);
  for (const sample of codecSamples) {
    const codec = standardCodecs.get(sample.value.tag);
    assert.ok(codec, `missing codec for ${sample.value.tag}`);
    const value =
      sample.value.tag === sdk.StandardContent_Tags.ReadReceipt
        ? undefined
        : sample.value.tag === sdk.StandardContent_Tags.Reaction ||
            sample.value.tag === sdk.StandardContent_Tags.Reply ||
            sample.value.tag === sdk.StandardContent_Tags.DeleteMessage
          ? sample.value
          : sample.value.inner[0];
    const encoded = codec.encode(value);
    assertEncodedEqual(encoded, sample.expected);
    assertEncodedEqual(codec.encode(codec.decode(encoded)), sample.expected);
  }
  console.log("Node P69: all 15 standard codecs match Rust bytes");

  const malformedDelete = sdk.StandardContent.DeleteMessage.new({
    messageId: "bad",
  });
  assert.throws(
    () => sdk.encodeStandard(malformedDelete),
    sdk.XmtpError.InvalidArgument,
  );
  assert.throws(
    () => new sdk.DeleteMessageCodec().encode(malformedDelete),
    sdk.XmtpError.InvalidArgument,
  );

  return codecSamples;
}
