// Node smokes for Rust behavior that reaches the app only through generated
// Node values: catch-up, metadata fields and attachments. Rust tests own the
// rules (client_build.rs, metadata_fields/*, attachment_flows.rs).
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  clientOptions,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
import {
  Client,
  Group,
  XmtpError,
  metadataFieldRef,
  type FieldValue,
} from "@xmtp/node-sdk";
import { expect, it } from "vitest";

const text = (value: string): FieldValue => ({ kind: "string", value });

it("catchUpToLive joins owed conversations once and reports bigint counts", async () => {
  const alix = await createRegisteredClient(createSigner().signer);
  const bo = await createRegisteredClient(createSigner().signer);
  const group = await bo.conversations.createGroup([alix.inboxId]);
  const sent = [await group.sendText("owed 1"), await group.sendText("owed 2")];
  const first = await alix.catchUpToLive(undefined);
  expect(first.completed).toBe(true);
  expect(typeof first.messages).toBe("bigint");
  expect(first.conversations).toBeGreaterThanOrEqual(1n);
  const joined = await alix.conversations.getById(group.id);
  if (!(joined instanceof Group))
    throw new Error("catch-up did not join the group");
  const ids = (await joined.messages()).map((message) => message.id);
  expect(sent.every((id) => ids.includes(id))).toBe(true);
  const again = await alix.catchUpToLive(undefined);
  expect(again).toEqual({
    messages: 0n,
    conversations: 0n,
    failed: 0n,
    completed: true,
  });
  await bo.end();
  await alix.end();
});

// verifies: META-069
it("well-known catalogue fields read and write between two Node clients", async () => {
  const alix = await createRegisteredClient(createSigner().signer);
  const bo = await createRegisteredClient(createSigner().signer);
  const group = await alix.conversations.createGroup([bo.inboxId]);
  const groupName = metadataFieldRef("groupName");
  const displayName = metadataFieldRef("userDisplayName");
  expect(displayName).toEqual({
    componentId: 0x800c,
    name: "USER_DISPLAY_NAME",
  });
  const fields = await group.metadataFields();
  const described = (field: typeof groupName) =>
    fields.find(
      (descriptor) => descriptor.field.componentId === field.componentId,
    );
  expect(described(groupName)).toMatchObject({
    field: groupName,
    componentType: { kind: "string" },
    isUserField: false,
  });
  expect(described(displayName)).toMatchObject({
    componentType: { kind: "map", keyType: "inboxId", valueType: "string" },
    isUserField: true,
  });
  await group.updateMetadataField(groupName, {
    kind: "replace",
    value: text("Team"),
  });
  await group.updateUserData([{ field: displayName, value: text("Alix") }]);
  await bo.conversations.sync();
  const peer = await bo.conversations.getById(group.id);
  if (!(peer instanceof Group)) throw new Error("Bo did not join the group");
  await peer.sync();
  expect(await peer.metadataValue(groupName)).toEqual({
    kind: "scalar",
    value: text("Team"),
  });
  expect(await peer.userData([displayName], [alix.inboxId])).toEqual(
    new Map([[alix.inboxId, [{ field: displayName, value: text("Alix") }]]]),
  );
  await expect(
    peer.updateMetadataField(
      { componentId: 0xc0ff, name: undefined },
      { kind: "replace", value: text("x") },
    ),
  ).rejects.toSatisfy(
    (error) =>
      error instanceof XmtpError.UnknownField &&
      error.details.code === "UnknownField" &&
      error.details.category === "input",
  );
  await bo.end();
  await alix.end();
});

// verifies: ATCH-043, ATCH-047, ATCH-062
it("a peer downloads an uploaded attachment to a file and deletes it", async () => {
  const root = await mkdtemp(join(tmpdir(), "xmtp-node-attachments-"));
  // The local object store is on a loopback address.
  const files = (name: string) =>
    clientOptions({
      storage: { location: { directory: join(root, name) } },
      attachments: { allowPrivateNetwork: true },
    });
  try {
    const sender = await Client.create(createSigner().signer, files("sender"));
    const receiver = await Client.create(
      createSigner().signer,
      files("receiver"),
    );
    expect(sender.attachments.offered).toBe(true);
    const source = (text: string) => ({
      kind: "bytes" as const,
      bytes: new TextEncoder().encode(text),
      filename: "note.txt",
      mimeType: "text/plain",
    });
    const pending = await sender.attachments.create(source("attachment bytes"));
    const other = await sender.attachments.create(source("other bytes"));
    const dm = await sender.conversations.createDm(receiver.inboxId);
    const sent = await dm.sendRemoteAttachment(pending.remoteAttachment);
    await pending.upload();
    await other.upload();
    expect(await pending.status()).toEqual({ kind: "complete" });

    await receiver.conversations.syncAll(undefined);
    const message = await receiver.conversations.getMessageById(sent);
    if (message?.content.kind !== "remoteAttachment")
      throw new Error("the record did not arrive");
    const attachments = receiver.attachments;
    const path = await attachments.localPath(message.content.value);
    const downloaded = await attachments.download(message.content.value);
    expect(downloaded).toEqual({
      path,
      mimeType: "text/plain",
      filename: "note.txt",
    });
    expect(await readFile(path, "utf8")).toBe("attachment bytes");
    // Another object under this digest fails typed, with its failure record.
    const substituted = {
      ...other.remoteAttachment,
      contentDigest: message.content.value.contentDigest,
    };
    const failure = await attachments.download(substituted).then(
      () => undefined,
      (error: unknown) => error,
    );
    expect(failure).toBeInstanceOf(XmtpError.Attachment);
    expect(failure).toMatchObject({
      details: { code: "Attachment" },
      attachmentFailure: { cause: "digestMismatch", retryable: false },
    });
    expect(await attachments.listLocal()).toHaveLength(1);
    await attachments.deleteLocal(message.content.value);
    expect(await attachments.listLocal()).toEqual([]);
    await expect(readFile(path)).rejects.toMatchObject({ code: "ENOENT" });
    await receiver.end();
    await sender.end();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
