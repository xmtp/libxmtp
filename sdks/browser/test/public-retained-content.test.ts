import type { ContentCodec, Group } from "@xmtp/browser-sdk";
import {
  ActionsCodec,
  AttachmentCodec,
  IntentCodec,
  MarkdownCodec,
  MultiRemoteAttachmentCodec,
  RemoteAttachmentCodec,
  Timestamp,
  TextCodec,
  TransactionReferenceCodec,
  WalletSendCallsCodec,
  initPureWasm,
} from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

import { create } from "./helpers";

beforeAll(() => initPureWasm());

function fixture<T>(
  name: string,
  makeCodec: () => ContentCodec<T>,
  value: T,
  kind: string,
) {
  return {
    name,
    expected: { kind, value },
    type: () => makeCodec().type,
    send: (group: Group) =>
      group.send(makeCodec(), value, { shouldPush: false }),
  };
}

const attachment = {
  mimeType: "image/png",
  content: new Uint8Array([1, 2, 3]),
};
const remote = {
  url: "https://example.com/file",
  scheme: "https",
  contentDigest: "digest",
  secret: new Uint8Array(32),
  salt: new Uint8Array(32),
  nonce: new Uint8Array(12),
  contentLength: 3,
};
const transaction = { networkId: "1", reference: "0x123" };
const wallet = {
  version: "1.0",
  chainId: "0x1",
  from: "0xabc",
  calls: [{ to: "0xdef", data: "0x01", value: "0x1", gas: "0x5208" }],
};
const actions = {
  id: "actions",
  description: "Choose an action",
  actions: (["primary", "secondary", "danger"] as const).map((style) => ({
    id: style,
    label: style,
    style,
  })),
};

const cases = [
  fixture("text", () => new TextCodec(), "Hello, world!", "text"),
  fixture("markdown", () => new MarkdownCodec(), "**message**", "markdown"),
  fixture(
    "attachment without filename",
    () => new AttachmentCodec(),
    attachment,
    "attachment",
  ),
  fixture(
    "attachment with filename",
    () => new AttachmentCodec(),
    { ...attachment, filename: "image.png" },
    "attachment",
  ),
  fixture(
    "remote attachment without filename",
    () => new RemoteAttachmentCodec(),
    remote,
    "remoteAttachment",
  ),
  fixture(
    "remote attachment with filename",
    () => new RemoteAttachmentCodec(),
    { ...remote, filename: "image.png" },
    "remoteAttachment",
  ),
  fixture(
    "one remote attachment",
    () => new MultiRemoteAttachmentCodec(),
    { attachments: [remote] },
    "multiRemoteAttachment",
  ),
  fixture(
    "two remote attachments",
    () => new MultiRemoteAttachmentCodec(),
    {
      attachments: [
        remote,
        { ...remote, filename: "other.png", url: "https://example.com/other" },
      ],
    },
    "multiRemoteAttachment",
  ),
  fixture(
    "transaction without namespace",
    () => new TransactionReferenceCodec(),
    transaction,
    "transactionReference",
  ),
  fixture(
    "transaction with namespace",
    () => new TransactionReferenceCodec(),
    { ...transaction, namespace: "eip155" },
    "transactionReference",
  ),
  fixture(
    "transaction with empty reference",
    () => new TransactionReferenceCodec(),
    { ...transaction, reference: "" },
    "transactionReference",
  ),
  fixture(
    "transaction metadata",
    () => new TransactionReferenceCodec(),
    {
      ...transaction,
      metadata: {
        transactionType: "transfer",
        currency: "ETH",
        amount: 1,
        decimals: 18,
        fromAddress: "0xabc",
        toAddress: "0xdef",
      },
    },
    "transactionReference",
  ),
  fixture(
    "one wallet call",
    () => new WalletSendCallsCodec(),
    wallet,
    "walletSendCalls",
  ),
  fixture(
    "multiple wallet calls",
    () => new WalletSendCallsCodec(),
    { ...wallet, calls: [...wallet.calls, { to: "0x123", data: "0x02" }] },
    "walletSendCalls",
  ),
  fixture(
    "wallet metadata and capabilities",
    () => new WalletSendCallsCodec(),
    {
      ...wallet,
      calls: [
        {
          ...wallet.calls[0],
          metadata: {
            description: "Transfer",
            transactionType: "transfer",
            extra: new Map([["currency", "ETH"]]),
          },
        },
      ],
      capabilities: new Map([
        ["paymasterService", '{"url":"https://example.com"}'],
      ]),
    },
    "walletSendCalls",
  ),
  fixture("all action styles", () => new ActionsCodec(), actions, "actions"),
  fixture(
    "action expiry and image",
    () => new ActionsCodec(),
    {
      ...actions,
      expiresAt: new Timestamp(1_234_000_000n),
      actions: actions.actions.map((action) => ({
        ...action,
        imageUrl: "https://example.com/image.png",
        expiresAt: new Timestamp(2_345_000_000n),
      })),
    },
    "actions",
  ),
  fixture(
    "intent without metadata",
    () => new IntentCodec(),
    { id: "intent", actionId: "primary" },
    "intent",
  ),
  fixture(
    "intent metadata",
    () => new IntentCodec(),
    { id: "intent", actionId: "primary", metadataJson: '{"choice":1}' },
    "intent",
  ),
];

test.each(cases)(
  "the real peer worker retains $name on list and lookup",
  async ({ send, expected, type }) => {
    const sender = await create();
    const peer = await create();
    const group = await sender.conversations.createGroup([peer.inboxId]);
    const id = await send(group);
    await peer.conversations.syncAll(undefined);
    const received = await peer.conversations.getById(group.id);
    if (!received) throw new Error("Peer group missing");
    for (const message of [
      await peer.conversations.getMessageById(id),
      (await received.messages()).find((item) => item.id === id),
    ]) {
      expect(message?.id).toBe(id);
      expect(message?.senderInboxId).toBe(sender.inboxId);
      expect(message?.conversationId).toBe(group.id);
      expect(message?.contentType).toEqual(type());
      expect(message?.content).toEqual(expected);
    }
  },
);

test("group update messages retain exact added, removed, and metadata fields", async () => {
  const client = await create();
  const peer = await create();
  const added = await create();
  const group = await client.conversations.createGroup([peer.inboxId]);
  const updates = async () =>
    (await group.messages()).filter(
      (message) => message.content.kind === "groupUpdated",
    );
  expect(
    (await updates()).some(
      (message) =>
        message.content.kind === "groupUpdated" &&
        message.content.value.addedInboxes.includes(peer.inboxId),
    ),
  ).toBe(true);
  await group.addMembers([added.inboxId]);
  expect(
    (await updates()).some(
      (message) =>
        message.content.kind === "groupUpdated" &&
        message.content.value.initiatedByInboxId === client.inboxId &&
        message.content.value.addedInboxes.includes(added.inboxId),
    ),
  ).toBe(true);
  await group.removeMembers([added.inboxId]);
  expect(
    (await updates()).some(
      (message) =>
        message.content.kind === "groupUpdated" &&
        message.content.value.removedInboxes.includes(added.inboxId),
    ),
  ).toBe(true);
  await group.updateName("Updated group name");
  expect(
    (await updates()).some(
      (message) =>
        message.content.kind === "groupUpdated" &&
        message.content.value.metadataFieldChanges.some(
          (field) => field.newValue === "Updated group name",
        ),
    ),
  ).toBe(true);
});
