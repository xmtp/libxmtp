import * as sdk from "../../../../target/sdk-pure-conformance/typescript-pure/index";

// verifies: P69
export async function checkPureCodecs(): Promise<number> {
  const loading = sdk.initPureWasm();
  if (loading !== sdk.initPureWasm()) throw new Error("pure WASM loaded twice");
  await loading;
  const codecs = new Map([
    [sdk.StandardContent_Tags.Text, new sdk.TextCodec()],
    [sdk.StandardContent_Tags.Markdown, new sdk.MarkdownCodec()],
    [sdk.StandardContent_Tags.ReadReceipt, new sdk.ReadReceiptCodec()],
    [sdk.StandardContent_Tags.Reaction, new sdk.ReactionV2Codec()],
    [sdk.StandardContent_Tags.Attachment, new sdk.AttachmentCodec()],
    [sdk.StandardContent_Tags.RemoteAttachment, new sdk.RemoteAttachmentCodec()],
    [sdk.StandardContent_Tags.MultiRemoteAttachment, new sdk.MultiRemoteAttachmentCodec()],
    [sdk.StandardContent_Tags.TransactionReference, new sdk.TransactionReferenceCodec()],
    [sdk.StandardContent_Tags.WalletSendCalls, new sdk.WalletSendCallsCodec()],
    [sdk.StandardContent_Tags.Actions, new sdk.ActionsCodec()],
    [sdk.StandardContent_Tags.Intent, new sdk.IntentCodec()],
    [sdk.StandardContent_Tags.Reply, new sdk.ReplyCodec()],
    [sdk.StandardContent_Tags.GroupUpdated, new sdk.GroupUpdatedCodec()],
    [sdk.StandardContent_Tags.DeleteMessage, new sdk.DeleteMessageCodec()],
    [sdk.StandardContent_Tags.LeaveRequest, new sdk.LeaveRequestCodec()],
  ]);
  const samples = sdk.sdkConformanceStandardSamples();
  if (samples.length !== 15) throw new Error(`expected 15 samples, got ${samples.length}`);
  for (const sample of samples) {
    const codec = codecs.get(sample.value.tag);
    if (!codec) throw new Error(`missing codec ${sample.value.tag}`);
    const value =
      sample.value.tag === sdk.StandardContent_Tags.ReadReceipt
        ? undefined
        : sample.value.tag === sdk.StandardContent_Tags.Reaction ||
            sample.value.tag === sdk.StandardContent_Tags.Reply ||
            sample.value.tag === sdk.StandardContent_Tags.DeleteMessage
          ? sample.value
          : sample.value.inner[0];
    const encoded = codec.encode(value as never);
    const roundTrip = codec.encode(codec.decode(encoded) as never);
    const expected = Array.from(new Uint8Array(sample.expected.content));
    if (
      JSON.stringify(Array.from(new Uint8Array(encoded.content))) !==
        JSON.stringify(expected) ||
      JSON.stringify(Array.from(new Uint8Array(roundTrip.content))) !==
        JSON.stringify(expected)
    ) {
      throw new Error(`codec bytes differ for ${sample.value.tag}`);
    }
    const parameters = (value: Map<string, string>): string =>
      JSON.stringify([...value].sort(([left], [right]) => left.localeCompare(right)));
    if (parameters(encoded.parameters) !== parameters(sample.expected.parameters)) {
      throw new Error(`codec parameters differ for ${sample.value.tag}`);
    }
    if (
      encoded.type.typeID !== sample.expected.type.typeID ||
      encoded.fallback !== sample.expected.fallback
    ) {
      throw new Error(`codec metadata differs for ${sample.value.tag}`);
    }
  }
  return samples.length;
}
