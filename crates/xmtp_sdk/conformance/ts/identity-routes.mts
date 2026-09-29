import assert from "node:assert/strict";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { isInvalidId } from "./node-codecs.mts";

async function members(group: sdk.GroupLike): Promise<string[]> {
  return (await group.members()).map((member) => member.inboxId).sort();
}

// Each membership union routes inbox IDs and account identities to the same
// change. An empty list uses inbox IDs. A mixed list fails before any call.
export async function checkIdentityRoutes(
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
  try {
    const conversations = a.conversations();
    const empty = await conversations.createGroup([], undefined);
    assert.deepEqual(await members(empty), [a.inboxId()]);
    const pair = [a.inboxId(), b.inboxId()].sort();
    const group = await conversations.createGroup([b.identity()], undefined);
    assert.deepEqual(await members(group), pair);
    await group.removeMembers([b.identity()]);
    assert.deepEqual(await members(group), [a.inboxId()]);
    const added = await group.addMembers([b.identity()]);
    assert.deepEqual(added.added, [b.inboxId()]);
    assert.deepEqual(await members(group), pair);
    await group.removeMembers([b.inboxId()]);
    await group.addMembers([b.inboxId()]);
    assert.deepEqual(await members(group), pair);
    const dm = await conversations.createDm(b.identity(), undefined);
    assert.equal(await dm.peerInboxId(), b.inboxId());
    assert.equal(
      (await conversations.createDm(b.inboxId(), undefined)).id(),
      dm.id(),
    );
    const before = (await conversations.list(undefined)).length;
    const mixed = [b.inboxId(), b.identity()] as unknown as string[];
    await assert.rejects(
      conversations.createGroup(mixed, undefined),
      isInvalidId,
    );
    await assert.rejects(group.addMembers(mixed), isInvalidId);
    assert.equal((await conversations.list(undefined)).length, before);
  } finally {
    await b.end();
    await a.end();
  }
  console.log("Node identity unions route inbox IDs and account identities");
}
