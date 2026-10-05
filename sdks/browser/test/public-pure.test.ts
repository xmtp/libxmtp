import {
  ActionsCodec,
  AttachmentCodec,
  IntentCodec,
  RemoteAttachmentCodec,
  Timestamp,
  WalletSendCallsCodec,
  initPureWasm,
} from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

beforeAll(() => initPureWasm());

test("attachment codecs preserve optional filename and transfer fields", () => {
  const attachment = {
    mimeType: "image/png",
    content: new Uint8Array([1, 2, 3]),
  };
  const local = new AttachmentCodec();
  expect(local.decode(local.encode(attachment))).toEqual(attachment);
  expect(
    local.decode(local.encode({ ...attachment, filename: "image.png" })),
  ).toEqual({ ...attachment, filename: "image.png" });
  const remote = new RemoteAttachmentCodec();
  const value = {
    url: "https://example.com/file",
    scheme: "https",
    contentDigest: "digest",
    secret: new Uint8Array(32),
    salt: new Uint8Array(32),
    nonce: new Uint8Array(12),
    contentLength: 3,
  };
  expect(remote.decode(remote.encode(value))).toEqual(value);
  expect(
    remote.decode(remote.encode({ ...value, filename: "image.png" })),
  ).toEqual({ ...value, filename: "image.png" });
});

test("actions and intents preserve all styles, expiry, image, and metadata", () => {
  const codec = new ActionsCodec();
  const value = {
    id: "actions",
    description: "Choose an action",
    expiresAt: new Timestamp(1_234_000_000n),
    actions: (["primary", "secondary", "danger"] as const).map((style) => ({
      id: style,
      label: style,
      style,
      imageUrl: "https://example.com/image.png",
      expiresAt: new Timestamp(2_345_000_000n),
    })),
  };
  expect(codec.decode(codec.encode(value))).toEqual(value);
  const intent = new IntentCodec();
  expect(
    intent.decode(intent.encode({ id: "intent", actionId: "primary" })),
  ).toEqual({ id: "intent", actionId: "primary" });
  const detailed = {
    id: "intent",
    actionId: "primary",
    metadataJson: '{"choice":1}',
  };
  expect(intent.decode(intent.encode(detailed))).toEqual(detailed);
});

test("wallet calls preserve metadata and capabilities and reject missing required fields", () => {
  const codec = new WalletSendCallsCodec();
  const value = {
    version: "1.0",
    chainId: "0x1",
    from: "0xabc",
    calls: [
      {
        to: "0xdef",
        data: "0x01",
        value: "0x1",
        gas: "0x5208",
        metadata: {
          description: "Transfer",
          transactionType: "transfer",
          extra: new Map([["currency", "ETH"]]),
        },
      },
      { to: "0x123", data: "0x02" },
    ],
    capabilities: new Map([
      ["paymasterService", '{"url":"https://example.com"}'],
    ]),
  };
  expect(codec.decode(codec.encode(value))).toEqual(value);
  for (const field of ["description", "transactionType"] as const) {
    const metadata = { ...value.calls[0].metadata! };
    delete (metadata as Partial<typeof metadata>)[field];
    expect(() =>
      codec.encode({ ...value, calls: [{ ...value.calls[0], metadata }] }),
    ).toThrow();
  }
});
