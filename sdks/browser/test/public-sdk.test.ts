import {
  Client,
  Timestamp,
  XmtpError,
  latestInboxUpdatesCount,
} from "@xmtp/browser-sdk";
import { initPureWasm, TextCodec } from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test, vi } from "vitest";

import { backend, create, options, signer } from "./helpers";

beforeAll(() => initPureWasm());

// verifies: IDENT-072
// verifies: SEND-020
test("public package registers, queries identities, sends, and closes", async () => {
  const owner = signer();
  const client = await create(owner);
  expect(await client.isRegistered()).toBe(true);
  expect(client.identity).toEqual(await owner.identity());
  expect(await Client.inboxIdFor(client.identity, backend)).toBe(
    client.inboxId,
  );
  expect(
    (await Client.canMessage([client.identity], backend)).get(
      `ethereum:${client.identity.identifier}`,
    ),
  ).toBe(true);
  expect(
    (await Client.inboxStates([client.inboxId], backend))[0].installations.map(
      (item) => item.id,
    ),
  ).toContain(client.installationId);
  expect(
    (await latestInboxUpdatesCount([client.inboxId], backend)).get(
      client.inboxId,
    ),
  ).toBeGreaterThan(0n);
  expect(
    (await client.keyPackageStatuses([client.installationId])).get(
      client.installationId,
    )?.validationError,
  ).toBeUndefined();
  const group = await client.conversations.createGroup([]);
  const id = await group.sendText("public package", { shouldPush: true });
  const message = await client.conversations.getMessageById(id);
  expect(message?.content).toEqual({ kind: "text", value: "public package" });
  expect(message?.client()).toBe(client);
  expect(message?.sentAt).toBeInstanceOf(Timestamp);
  expect(message?.rawBytes).toBeInstanceOf(Uint8Array);
  await client.end();
  await expect(group.messages()).rejects.toBeInstanceOf(XmtpError.ClientClosed);
});

test("a DM created from an identity keeps optimistic sends, filters, reactions, and replies", async () => {
  const client = await create();
  const peer = await create();
  const dm = await client.conversations.createDm(peer.identity);
  expect((await dm.state()).pausedForVersion).toBeUndefined();
  const id = await dm.sendText("first", { optimistic: true, shouldPush: true });
  await dm.publishMessages();
  const text = new TextCodec().encode("reply");
  await dm.sendReply(id, client.inboxId, text, { shouldPush: true });
  await dm.sendReaction(
    id,
    client.inboxId,
    { action: "added", schema: "unicode", content: "👍" },
    { shouldPush: true },
  );
  const messages = await dm.messages();
  expect(
    messages.some(
      (message) => message.id === id && message.content.kind === "text",
    ),
  ).toBe(true);
  expect(
    messages.some(
      (message) =>
        message.content.kind === "reply" &&
        message.content.referenceId === id &&
        message.content.body.kind === "text" &&
        message.content.body.value === "reply",
    ),
  ).toBe(true);
  expect((await dm.messages({ limit: 1 })).length).toBe(1);
  expect(
    await dm.messages({ sentAfter: new Timestamp(2n ** 63n - 1n) }),
  ).toHaveLength(0);
});

test("stream callbacks observe new conversations and messages", async () => {
  const client = await create();
  const conversations: string[] = [];
  const messages: string[] = [];
  const conversationStream = client.conversations.stream();
  const messageStream = client.conversations.streamAllMessages();
  await conversationStream.ready();
  await messageStream.ready();
  const conversationRead = conversationStream.onValue((conversation) => {
    conversations.push(conversation.id);
  });
  const messageRead = messageStream.onValue((message) => {
    messages.push(message.id);
  });
  try {
    const group = await client.conversations.createGroup([]);
    const id = await group.sendText("stream", { shouldPush: true });
    await vi.waitFor(
      () => {
        expect(conversations).toContain(group.id);
        expect(messages).toContain(id);
      },
      { timeout: 30_000 },
    );
  } finally {
    await conversationStream.end();
    await messageStream.end();
    await conversationRead;
    await messageRead;
  }
});

// verifies: ARCH-012
test("byte archives reject invalid keys and restore messages", async () => {
  const owner = signer();
  const client = await create(owner);
  const group = await client.conversations.createGroup([]);
  const id = await group.sendText("archive");
  for (const length of [31, 33])
    await expect(
      client.archives.exportToBytes(new Uint8Array(length), undefined),
    ).rejects.toThrow();
  const key = new Uint8Array(32).fill(1);
  const archive = await client.archives.exportToBytes(key, undefined);
  expect(
    (await client.archives.metadataFromBytes(archive, key)).elements.length,
  ).toBeGreaterThan(0);
  const replacement = await create(owner);
  await replacement.archives.importFromBytes(archive, key);
  const restored = await replacement.conversations.getById(group.id);
  expect(
    (await restored!.messages()).some((message) => message.id === id),
  ).toBe(true);
});

test("configuration snapshots and diagnostic counters remain public", async () => {
  const client = await create();
  const snapshot = client.serverConfiguration;
  expect(snapshot.identifier).not.toBe("");
  expect(Object.keys(snapshot).sort()).toEqual(
    [
      "identifier",
      "serverVersion",
      "minLibxmtpVersion",
      "auth",
      "retention",
      "limits",
      "mls",
      "smartContractWalletChains",
      "attachments",
      "applicationComponents",
    ].sort(),
  );
  expect(Object.keys(snapshot.auth).sort()).toEqual(
    ["enabled", "keys", "audiences", "issuers", "requiredScopes"].sort(),
  );
  for (const value of Object.values(snapshot.retention)) {
    expect(value).toBeTypeOf("bigint");
    expect(value).toBeGreaterThan(0n);
  }
  const rateFields = new Set([
    "maxUpdateFramesPerSecond",
    "maxUpdateBurst",
    "maxPingFramesPerSecond",
    "maxPingBurst",
  ]);
  expect(Object.keys(snapshot.limits)).toHaveLength(20);
  for (const [key, value] of Object.entries(snapshot.limits)) {
    expect(value).toBeTypeOf(rateFields.has(key) ? "number" : "bigint");
    expect(value).toBeGreaterThan(0);
  }
  expect(snapshot.mls.maxGroupMembers).toBeGreaterThan(0n);
  expect(snapshot.mls.maxInstallationsPerInbox).toBe(10n);
  expect(Array.isArray(snapshot.smartContractWalletChains)).toBe(true);

  expect((await Client.fetchServerConfiguration(backend)).identifier).toBe(
    snapshot.identifier,
  );
  expect((await client.refreshServerConfiguration()).identifier).toBe(
    snapshot.identifier,
  );
  expect(client.serverConfiguration).toEqual(snapshot);
  const counters = await client.diagnostics.apiStatistics();
  expect(counters.publish).toBeGreaterThanOrEqual(2n);
  expect(counters.queryNewest).toBeGreaterThanOrEqual(1n);
  expect(counters.query).toBeGreaterThanOrEqual(2n);
  expect(counters.subscribe).toBe(0n);
  expect(counters.subscribeStatic).toBe(0n);
  expect(
    (await client.diagnostics.identityStatistics())
      .verifySmartContractWalletSignatures,
  ).toBe(0n);
  expect(
    (await client.diagnostics.identityStatistics()).getInboxIds,
  ).toBeGreaterThanOrEqual(1n);
  await client.diagnostics.clearStatistics();
  expect(
    (await client.diagnostics.apiStatistics()).publish,
  ).toBeLessThanOrEqual(1n);
  const cleared = await client.diagnostics.apiStatistics();
  for (const field of [
    "publish",
    "query",
    "queryNewest",
    "subscribe",
    "subscribeStatic",
  ] as const)
    expect(cleared[field]).toBeLessThanOrEqual(1n);
  expect(
    (await client.diagnostics.identityStatistics()).getInboxIds,
  ).toBeLessThanOrEqual(1n);
  expect(
    (await client.diagnostics.identityStatistics())
      .verifySmartContractWalletSignatures,
  ).toBe(0n);
  expect(await client.diagnostics.aggregateStatistics()).toBeTypeOf("string");
});

test("a codec imported from pure encodes once when sent through the root", async () => {
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const codec = new TextCodec();
  const encode = vi.spyOn(codec, "encode");
  await group.send(codec, "one encode", { shouldPush: true });
  expect(encode).toHaveBeenCalledExactlyOnceWith("one encode");
});

test("standard codec subclasses keep app fallback hooks before publication", async () => {
  class CustomText extends TextCodec {
    override fallback(value: string): string {
      return `custom ${value}`;
    }
  }
  class FailedText extends TextCodec {
    override fallback(_value: string): string {
      throw new Error("app fallback");
    }
  }
  const client = await create();
  const group = await client.conversations.createGroup([]);
  const id = await group.send(new CustomText(), "override", {
    shouldPush: true,
  });
  expect((await client.conversations.getMessageById(id))?.fallback).toBe(
    "custom override",
  );
  const before = await group.countMessages({ kind: "application" });
  await expect(
    group.send(new FailedText(), "never publish"),
  ).rejects.toBeInstanceOf(XmtpError.CodecEncodeFailed);
  expect(await group.countMessages({ kind: "application" })).toBe(before);
});

test("a signerless build needs registration and keeps byte signatures typed", async () => {
  const owner = signer();
  const client = await create(owner);
  await expect(
    Client.build(await owner.identity(), options),
  ).rejects.toBeInstanceOf(XmtpError.IdentityNotFound);
  const storage = {
    location: { directory: `signerless-${crypto.randomUUID()}` },
  } as const;
  const persistent = await create(owner, { storage });
  const installation = persistent.installationId;
  await persistent.end();
  const built = await Client.build(await owner.identity(), {
    ...options,
    storage,
  });
  try {
    expect(built.identity).toEqual(client.identity);
    expect(built.installationId).toBe(installation);
    expect(await built.isRegistered()).toBe(true);
  } finally {
    await built.end();
  }
  expect(await client.ownInboxUpdatesCount(true)).toBeGreaterThan(0n);
  const signed = await client.signWithInstallationKey("signature");
  expect(signed).toBeInstanceOf(Uint8Array);
  expect(
    await client.verifySignedWithInstallationKey("signature", signed),
  ).toBe(true);
  expect(await client.verifySignedWithInstallationKey("changed", signed)).toBe(
    false,
  );
  expect(
    await Client.verifySignedWithPublicKey(
      "signature",
      signed,
      client.installationIdBytes,
    ),
  ).toBe(true);
});
