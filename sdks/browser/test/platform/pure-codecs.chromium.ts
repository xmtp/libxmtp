// The public pure module in real Chromium: each standalone codec encodes the
// conformance sample to the same bytes as Rust, over public values, and ID and
// input failures throw the public XmtpError.
import * as sdk from "../../../../target/sdk-pure-codec-fixture/typescript-pure/index";

type Kind = sdk.StandardContent["kind"];

// The codec value of a sample: the variant's value, or the whole variant for
// the codecs that encode a standard content variant.
function codecValue(content: sdk.StandardContent): unknown {
  switch (content.kind) {
    case "readReceipt":
      return undefined;
    case "reaction":
    case "reply":
    case "deleteMessage":
      return content;
    default:
      return content.value;
  }
}

export async function checkPureCodecs(): Promise<number> {
  const loading = sdk.initPureWasm();
  if (loading !== sdk.initPureWasm()) throw new Error("pure WASM loaded twice");
  await loading;
  const codecs = new Map<Kind, sdk.ContentCodec<never>>([
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
  const samples = sdk.sdkConformanceStandardSamples();
  if (samples.length !== 15)
    throw new Error(`expected 15 samples, got ${samples.length}`);
  for (const sample of samples) {
    const kind = sample.value.kind;
    const codec = codecs.get(kind);
    if (!codec) throw new Error(`missing codec ${kind}`);
    const value = codecValue(sample.value);
    const encoded = codec.encode(value as never);
    if (!(encoded.content instanceof Uint8Array))
      throw new Error(`codec bytes are not a Uint8Array for ${kind}`);
    if (codec.fallback?.(value as never) !== sample.expected.fallback)
      throw new Error(`standalone fallback differs from Rust for ${kind}`);
    const push = sdk.catalogueContentTypeShouldPush(sample.expected.type);
    if (codec.shouldPush?.(value as never) !== push)
      throw new Error(
        `standalone push differs from Rust catalogue for ${kind}`,
      );
    if (
      kind === "leaveRequest" &&
      (push !== false ||
        sample.expected.fallback !== "A member has requested leaving the group")
    )
      throw new Error(
        "leave request lost its quiet policy or retained fallback",
      );
    const roundTrip = codec.encode(codec.decode(encoded) as never);
    const expected = JSON.stringify(Array.from(sample.expected.content));
    if (
      JSON.stringify(Array.from(encoded.content)) !== expected ||
      JSON.stringify(Array.from(roundTrip.content)) !== expected
    ) {
      throw new Error(`codec bytes differ for ${kind}`);
    }
    const parameters = (value: ReadonlyMap<string, string>): string =>
      JSON.stringify(
        [...value].sort(([left], [right]) => left.localeCompare(right)),
      );
    if (
      parameters(encoded.parameters) !== parameters(sample.expected.parameters)
    ) {
      throw new Error(`codec parameters differ for ${kind}`);
    }
    if (
      encoded.type.typeId !== sample.expected.type.typeId ||
      encoded.fallback !== sample.expected.fallback
    ) {
      throw new Error(`codec metadata differs for ${kind}`);
    }
  }

  const failsWith = (
    label: string,
    type:
      | typeof sdk.XmtpError.InvalidArgument
      | typeof sdk.XmtpError.InvalidInput,
    code: "InvalidArgument" | "InvalidInput",
    encode: () => unknown,
  ): void => {
    try {
      encode();
    } catch (error) {
      if (
        error instanceof type &&
        !("tag" in error) &&
        error.details.code === code &&
        error.details.category === "input" &&
        error.details.retryable === false
      ) {
        return;
      }
      throw new Error(
        `${label} did not throw the public ${code}: ${error instanceof Error ? `${error.name}: ${error.message}` : String(error)}`,
        { cause: error },
      );
    }
    throw new Error(`${label} accepted malformed input`);
  };
  const invalidArgument = (label: string, encode: () => unknown): void =>
    failsWith(label, sdk.XmtpError.InvalidArgument, "InvalidArgument", encode);
  const sample = <K extends Kind>(
    kind: K,
  ): Extract<sdk.StandardContent, { kind: K }> => {
    const value = samples.find((item) => item.value.kind === kind)?.value;
    if (value?.kind !== kind) throw new Error(`missing ${kind} sample`);
    return value as Extract<sdk.StandardContent, { kind: K }>;
  };

  const badDelete: sdk.StandardContent = {
    ...sample("deleteMessage"),
    messageId: "bad",
  };
  invalidArgument("encodeStandard delete ID", () =>
    sdk.encodeStandard(badDelete),
  );
  invalidArgument("DeleteMessageCodec delete ID", () =>
    new sdk.DeleteMessageCodec().encode(badDelete),
  );

  const reaction = sample("reaction");
  for (const [label, value] of [
    ["reaction reference", { ...reaction, reference: "bad" }],
    ["reaction inbox", { ...reaction, referenceInboxId: "" }],
  ] as const) {
    invalidArgument(label, () => new sdk.ReactionV2Codec().encode(value));
  }

  const reply = sample("reply");
  for (const [label, value] of [
    ["reply reference", { ...reply, reference: "bad" }],
    ["reply inbox", { ...reply, referenceInboxId: "" }],
  ] as const) {
    invalidArgument(label, () => new sdk.ReplyCodec().encode(value));
  }

  const groupValue = sample("groupUpdated").value;
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
  failsWith(
    "intent metadata JSON",
    sdk.XmtpError.InvalidInput,
    "InvalidInput",
    () =>
      sdk.encodeStandard({
        kind: "intent",
        value: { id: "intent", actionId: "action", metadataJson: "{bad json" },
      }),
  );
  return samples.length;
}
