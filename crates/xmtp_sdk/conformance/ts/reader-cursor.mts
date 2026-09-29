import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";

// verifies: PROC-033, PROC-034, PROC-050
export async function checkReaderCursor(
  signer: sdk.Signer,
  backend: sdk.BackendOptions,
): Promise<void> {
  const options = sdk.ClientOptions.create({
    backend: sdk.BackendSource.Options.new({ options: backend }),
    storage: sdk.StorageOptions.create({
      location: sdk.StorageLocation.Path.new(
        join(await mkdtemp(join(tmpdir(), "f3-cursor-")), "client.db"),
      ),
    }),
    deviceSync: false,
  });
  let client = await sdk.Client.create(signer, options);
  const identity = await signer.identity();
  const inbox = client.inboxId();
  await client.conversations().sdkConformanceSeedDeliveryCursor();
  const group = await client.conversations().createGroup([], undefined);
  const groupId = group.id();
  const beginning = await client.conversations().beginningDeliveryCursor();
  const firstId = await group.sendText("large A", undefined);
  const first = (await group.messages(undefined)).find(
    (m) => m.id === firstId,
  )!;
  const cursor = first.deliveryCursor!;
  assert.ok(cursor.startsWith("dc1_"));
  assert.equal(
    Buffer.from(cursor.slice(4), "base64url").readBigUInt64BE(16),
    9007199254740993n,
  );
  assert.equal(
    (await client.conversations().getMessageById(firstId))?.deliveryCursor,
    cursor,
  );
  assert.equal((await first.refresh())?.deliveryCursor, cursor);
  const all = sdk.MessageStream.open(client, {
    from: beginning,
    consentStates: undefined,
    conversationKind: sdk.ConversationKind.Group,
  });
  assert.equal((await all.next()).value?.deliveryCursor, cursor);
  await all.end();
  const named = sdk.MessageStream.openGroup(client, group);
  assert.equal((await named.next()).value?.deliveryCursor, cursor);
  await named.end();
  const secondId = await group.sendText("large B", undefined);
  const resume = await group.messageReader({ from: cursor });
  const second = await resume.next();
  assert.equal(second?.id, secondId);
  assert.equal(
    Buffer.from(second!.deliveryCursor!.slice(4), "base64url").readBigUInt64BE(
      16,
    ),
    9007199254740994n,
  );
  await resume.end();
  const encoded = new sdk.TextCodec().encode("reply");
  const replyId = await group.sendReply(firstId, undefined, encoded, undefined);
  const reply = await client.conversations().getMessageById(replyId);
  assert.equal((await reply?.parent())?.deliveryCursor, cursor);
  const preparedId = await group.prepareMessage(encoded, undefined);
  assert.equal(
    (await client.conversations().getMessageById(preparedId))?.deliveryCursor,
    null,
  );
  await group.publishMessage(preparedId);
  assert.ok(
    (await client.conversations().getMessageById(preparedId))?.deliveryCursor,
  );
  await client.end();
  client = await sdk.Client.build(identity, options, inbox);
  const restored = await client.conversations().getById(groupId);
  assert.equal(restored?.tag, sdk.Conversation_Tags.Group);
  if (restored?.tag !== sdk.Conversation_Tags.Group)
    throw new Error("group missing");
  const replay = sdk.MessageStream.openGroup(client, restored.inner.group, {
    from: cursor,
  });
  const repeated = (await replay.next()).value;
  assert.equal(repeated?.id, secondId);
  assert.equal(repeated?.deliveryCursor, second?.deliveryCursor);
  await replay.end();
  await client.end();
  console.log(
    "Node F3 exact large cursor, full message, selection, and reopen passed",
  );
}

// verifies: DMS-017, PROC-034, PROC-050
export async function checkRestoredPeer(
  backend: sdk.BackendOptions,
): Promise<void> {
  const settings = sdk.ClientOptions.create({
    backend: sdk.BackendSource.Options.new({ options: backend }),
    storage: sdk.StorageOptions.create({
      location: sdk.StorageLocation.InMemory.new(),
    }),
    deviceSync: false,
  });
  const a = await sdk.Client.create(await sdk.generateLocalSigner(), settings);
  const b = await sdk.Client.create(await sdk.generateLocalSigner(), settings);
  const c = await sdk.Client.create(await sdk.generateLocalSigner(), settings);
  try {
    const dm = await a.conversations().createDm(b.inboxId());
    const other = await b.conversations().createDm(a.inboxId());
    assert.notEqual(dm.id(), other.id());
    assert.equal(await dm.peerInboxId(), b.inboxId());
    assert.equal(await other.peerInboxId(), a.inboxId());
    await a.conversations().syncAll(undefined);
    const id = await dm.sendText("foreign restored DM", undefined);
    const key = new Uint8Array(32).fill(9).buffer;
    const archive = await a
      .archives()
      .exportToBytes(
        key,
        sdk.ArchiveOptions.create({ elements: [sdk.ArchiveElement.Messages] }),
      );
    await c.archives().importFromBytes(archive, key);
    const conversation = await c.conversations().getById(dm.id());
    assert.equal(conversation?.tag, sdk.Conversation_Tags.Dm);
    if (conversation?.tag !== sdk.Conversation_Tags.Dm)
      throw new Error("DM missing");
    const restored = conversation.inner.dm;
    assert.equal(await restored.peerInboxId(), null);
    const listed = await c
      .conversations()
      .listDms(
        sdk.ListConversationsOptions.create({ includeDuplicateDms: true }),
      );
    assert.equal(listed.length, 2);
    for (const item of listed) assert.equal(await item.peerInboxId(), null);
    const duplicates = await restored.duplicateDms();
    assert.equal(duplicates.length, 1);
    assert.equal(await duplicates[0].peerInboxId(), null);
    const cursor = (await restored.messages(undefined)).find(
      (message) => message.id === id,
    )!.deliveryCursor;
    assert.ok(cursor);
    const stream = sdk.MessageStream.openDm(c, restored, {
      from: await c.conversations().beginningDeliveryCursor(),
    });
    const first = (await stream.next()).value;
    assert.equal(first?.id, id);
    assert.equal(first?.deliveryCursor, cursor);
    await stream.end();
    const reader = await restored.messageReader();
    assert.equal((await reader.next())?.deliveryCursor, cursor);
    await reader.end();
    await assert.rejects(
      restored.messageReader({ from: "invalid" }),
      sdk.XmtpError.InvalidCursor,
    );
    await assert.rejects(
      restored.messageReader({
        from: await a.conversations().beginningDeliveryCursor(),
      }),
      sdk.XmtpError.ForeignCursor,
    );
  } finally {
    await c.end();
    await b.end();
    await a.end();
  }
  console.log(
    "Node F3 Restored peer lookup/list/duplicates and Dm selection passed",
  );
}
