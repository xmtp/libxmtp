import * as sdk from "@xmtp/node-sdk";
const remote = {
  url: "https://example.test/file",
  contentDigest: "digest",
  secret: new Uint8Array(32).fill(1),
  salt: new Uint8Array(32).fill(2),
  nonce: new Uint8Array(12).fill(3),
  scheme: "https",
  contentLength: 10,
  filename: "file",
};
export const standardSamples: sdk.StandardContent[] = [
  { kind: "text", value: "hello" },
  { kind: "markdown", value: "**hello**" },
  { kind: "readReceipt" },
  {
    kind: "reaction",
    reference: "a".repeat(64),
    referenceInboxId: "inbox",
    reaction: { action: "added", content: "👍", schema: "unicode" },
  },
  {
    kind: "attachment",
    value: {
      mimeType: "text/plain",
      content: new TextEncoder().encode("file"),
    },
  },
  { kind: "remoteAttachment", value: remote },
  { kind: "multiRemoteAttachment", value: { attachments: [remote] } },
  { kind: "transactionReference", value: { networkId: "1", reference: "0x1" } },
  {
    kind: "walletSendCalls",
    value: { version: "1", chainId: "0x1", from: "0xsender", calls: [] },
  },
  {
    kind: "actions",
    value: {
      id: "actions",
      description: "Choose",
      actions: [{ id: "one", label: "One" }],
    },
  },
  { kind: "intent", value: { id: "actions", actionId: "one" } },
  {
    kind: "reply",
    reference: "a".repeat(64),
    referenceInboxId: "inbox",
    content: new sdk.TextCodec().encode("hello"),
  },
  {
    kind: "groupUpdated",
    value: {
      initiatedByInboxId: "inbox",
      addedInboxes: [],
      removedInboxes: [],
      leftInboxes: [],
      metadataFieldChanges: [],
      addedAdminInboxes: [],
      removedAdminInboxes: [],
      addedSuperAdminInboxes: [],
      removedSuperAdminInboxes: [],
    },
  },
  { kind: "deleteMessage", messageId: "a".repeat(64) },
  { kind: "leaveRequest", value: {} },
];

export const standardCodecs = [
  sdk.TextCodec,
  sdk.MarkdownCodec,
  sdk.ReadReceiptCodec,
  sdk.ReactionV2Codec,
  sdk.AttachmentCodec,
  sdk.RemoteAttachmentCodec,
  sdk.MultiRemoteAttachmentCodec,
  sdk.TransactionReferenceCodec,
  sdk.WalletSendCallsCodec,
  sdk.ActionsCodec,
  sdk.IntentCodec,
  sdk.ReplyCodec,
  sdk.GroupUpdatedCodec,
  sdk.DeleteMessageCodec,
  sdk.LeaveRequestCodec,
].map((C) => new C());

export const variantSamples: sdk.StandardContent[] = [
  {
    kind: "attachment",
    value: {
      filename: "image.png",
      mimeType: "image/png",
      content: new Uint8Array([0, 1, 2]),
    },
  },
  { kind: "remoteAttachment", value: { ...remote, filename: undefined } },
  {
    kind: "multiRemoteAttachment",
    value: {
      attachments: [
        remote,
        { ...remote, url: "https://example.test/second", filename: undefined },
      ],
    },
  },
  {
    kind: "transactionReference",
    value: { namespace: "eip155", networkId: "1", reference: "" },
  },
  {
    kind: "transactionReference",
    value: {
      networkId: "1",
      reference: "0x123",
      metadata: {
        transactionType: "transfer",
        currency: "ETH",
        amount: 1,
        decimals: 18,
        fromAddress: "0xsender",
        toAddress: "0xrecipient",
      },
    },
  },
  {
    kind: "walletSendCalls",
    value: {
      version: "1",
      chainId: "0x1",
      from: "0xsender",
      calls: [
        { to: "0xrecipient", data: "0x", value: "0x1" },
        {
          to: "0xsecond",
          gas: "0x5208",
          metadata: {
            description: "Pay",
            transactionType: "transfer",
            extra: new Map([["currency", "ETH"]]),
          },
        },
      ],
      capabilities: new Map([["atomic", "true"]]),
    },
  },
  {
    kind: "actions",
    value: {
      id: "choices",
      description: "Choose",
      expiresAt: new sdk.Timestamp(1_000_000_000n),
      actions: [
        {
          id: "first",
          label: "First",
          style: "primary",
          imageUrl: "https://example.test/image",
          expiresAt: new sdk.Timestamp(2_000_000_000n),
        },
        { id: "second", label: "Second", style: "secondary" },
        { id: "third", label: "Third", style: "danger" },
      ],
    },
  },
  {
    kind: "intent",
    value: {
      id: "choices",
      actionId: "first",
      metadataJson: '{"selected":true}',
    },
  },
];
