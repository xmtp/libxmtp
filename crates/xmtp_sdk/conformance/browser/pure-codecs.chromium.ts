import * as sdk from "../../../../target/sdk-pure-conformance/typescript-pure/index";

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
  const samples = sdk.sdkConformanceStandardSamples();
  if (samples.length !== 15)
    throw new Error(`expected 15 samples, got ${samples.length}`);
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
      JSON.stringify(
        [...value].sort(([left], [right]) => left.localeCompare(right)),
      );
    if (
      parameters(encoded.parameters) !== parameters(sample.expected.parameters)
    ) {
      throw new Error(`codec parameters differ for ${sample.value.tag}`);
    }
    if (
      encoded.type.typeId !== sample.expected.type.typeId ||
      encoded.fallback !== sample.expected.fallback
    ) {
      throw new Error(`codec metadata differs for ${sample.value.tag}`);
    }
  }

  const invalidArgument = (label: string, encode: () => unknown): void => {
    try {
      encode();
    } catch (error) {
      if (
        error instanceof Error &&
        sdk.XmtpError.InvalidArgument.instanceOf(error) &&
        error.inner[0].code === "InvalidArgument" &&
        error.inner[0].category === sdk.ErrorCategory.Input &&
        error.inner[0].retryable === false
      ) {
        return;
      }
      throw new Error(
        `${label} did not return typed InvalidArgument: ${error instanceof Error ? `${error.name}: ${error.message}` : String(error)}`,
        { cause: error },
      );
    }
    throw new Error(`${label} accepted a malformed ID`);
  };
  const sample = (tag: sdk.StandardContent_Tags): sdk.StandardContent => {
    const value = samples.find((item) => item.value.tag === tag)?.value;
    if (!value) throw new Error(`missing ${tag} sample`);
    return value;
  };

  const deleted = sample(sdk.StandardContent_Tags.DeleteMessage);
  if (deleted.tag !== sdk.StandardContent_Tags.DeleteMessage)
    throw new Error("wrong delete sample");
  const badDelete = sdk.StandardContent.DeleteMessage.new({
    ...deleted.inner,
    messageId: "bad",
  });
  invalidArgument("encodeStandard delete ID", () =>
    sdk.encodeStandard(badDelete),
  );
  invalidArgument("DeleteMessageCodec delete ID", () =>
    new sdk.DeleteMessageCodec().encode(badDelete),
  );

  const reaction = sample(sdk.StandardContent_Tags.Reaction);
  if (reaction.tag !== sdk.StandardContent_Tags.Reaction)
    throw new Error("wrong reaction sample");
  for (const [label, value] of [
    ["reaction reference", { ...reaction.inner, reference: "bad" }],
    ["reaction inbox", { ...reaction.inner, referenceInboxId: "" }],
  ] as const) {
    invalidArgument(label, () =>
      new sdk.ReactionV2Codec().encode(sdk.StandardContent.Reaction.new(value)),
    );
  }

  const reply = sample(sdk.StandardContent_Tags.Reply);
  if (reply.tag !== sdk.StandardContent_Tags.Reply)
    throw new Error("wrong reply sample");
  for (const [label, value] of [
    ["reply reference", { ...reply.inner, reference: "bad" }],
    ["reply inbox", { ...reply.inner, referenceInboxId: "" }],
  ] as const) {
    invalidArgument(label, () =>
      new sdk.ReplyCodec().encode(sdk.StandardContent.Reply.new(value)),
    );
  }

  const group = sample(sdk.StandardContent_Tags.GroupUpdated);
  if (group.tag !== sdk.StandardContent_Tags.GroupUpdated)
    throw new Error("wrong group update sample");
  const groupValue = group.inner[0];
  invalidArgument("group initiator", () =>
    new sdk.GroupUpdatedCodec().encode({
      ...groupValue,
      initiatedByInboxId: "",
    }),
  );
  for (const field of [
    "addedInboxes",
    "removedInboxes",
    "leftInboxes",
    "addedAdminInboxes",
    "removedAdminInboxes",
    "addedSuperAdminInboxes",
    "removedSuperAdminInboxes",
  ] as const) {
    invalidArgument(`group ${field}`, () =>
      new sdk.GroupUpdatedCodec().encode({
        ...groupValue,
        [field]: [""],
      }),
    );
  }
  try {
    sdk.encodeStandard(
      sdk.StandardContent.Intent.new({
        id: "intent",
        actionId: "action",
        metadataJson: "{bad json",
      }),
    );
    throw new Error("invalid metadata JSON was accepted");
  } catch (error) {
    if (
      !(error instanceof Error) ||
      !sdk.XmtpError.InvalidInput.instanceOf(error) ||
      error.inner[0].code !== "InvalidInput" ||
      error.inner[0].category !== sdk.ErrorCategory.Input ||
      error.inner[0].retryable !== false
    ) {
      throw new Error("non-ID codec error changed", { cause: error });
    }
  }
  return samples.length;
}
