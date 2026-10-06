// Node smokes for Rust behavior that reaches the app only through generated
// Node values: catch-up, metadata fields and attachments. Rust tests own the
// rules (client_build.rs, metadata_fields/*, attachment_flows.rs).
import {
  mkdtemp,
  readFile,
  realpath,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, sep } from "node:path";

import {
  clientOptions,
  createRegisteredClient,
  createSigner,
} from "@test/helpers";
import {
  Client,
  Group,
  Timestamp,
  XmtpError,
  metadataFieldRef,
  type ClientEvent,
  type FieldValue,
  type MetadataBasePolicy,
  type PendingAttachment,
} from "@xmtp/node-sdk";
import { expect, it, vi } from "vitest";

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
  // Each slot of ComponentPermissions lifts its own policy. Rust builds these
  // in xmtp_mls_common app_data/creation.rs (build_registry and
  // register_configured_fields) from the default policy set. No standard
  // field has different insert and update policies.
  const policies = (
    insert: MetadataBasePolicy,
    update: MetadataBasePolicy,
    remove: MetadataBasePolicy,
  ) => ({
    insert: { kind: "base", value: insert },
    update: { kind: "base", value: update },
    delete: { kind: "base", value: remove },
  });
  const allow = { kind: "allow" } as const;
  const admin = { kind: "allowIfAdmin" } as const;
  const superAdmin = { kind: "allowIfSuperAdmin" } as const;
  const self = { kind: "allowIfSelfOrNonMember" } as const;
  expect(described(displayName)?.permissions).toEqual(
    policies(self, self, self),
  );
  expect(described(groupName)?.permissions).toEqual(
    policies(allow, allow, superAdmin),
  );
  expect(
    described(metadataFieldRef("messageDisappearInNs"))?.permissions,
  ).toEqual(policies(admin, admin, superAdmin));
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
  // An empty field filter keeps every member; an empty member filter keeps
  // none. Absent filters lower to undefined, not to an empty list.
  expect(await peer.userData([], undefined)).toEqual(
    new Map([
      [alix.inboxId, []],
      [bo.inboxId, []],
    ]),
  );
  expect(await peer.userData(undefined, [])).toEqual(new Map());
  // A map delta lowers its tuple variant (value0, value1) and inbox ID key;
  // the map value lifts back with that key.
  await peer.updateMetadataField(displayName, {
    kind: "mapDelta",
    value: [
      {
        kind: "insert",
        value0: { kind: "inboxId", value: bo.inboxId },
        value1: text("Bo"),
      },
    ],
  });
  await group.sync();
  expect(
    await group.mapValue(displayName, { kind: "inboxId", value: bo.inboxId }),
  ).toEqual(text("Bo"));
  // A batch read lifts each value in request order; an unset field lifts to
  // an undefined value.
  const description = metadataFieldRef("groupDescription");
  const values = await group.metadataValues([
    displayName,
    groupName,
    description,
  ]);
  expect(values.map((value) => value.field.componentId)).toEqual([
    displayName.componentId,
    groupName.componentId,
    description.componentId,
  ]);
  expect(values[2]).toEqual({ field: description, value: undefined });
  const entry = (inboxId: string, name: string) => ({
    key: { kind: "inboxId", value: inboxId },
    value: text(name),
  });
  expect(values[0]!.value).toEqual({
    kind: "map",
    value: expect.arrayContaining([
      entry(alix.inboxId, "Alix"),
      entry(bo.inboxId, "Bo"),
    ]),
  });
  expect(values[0]!.value?.value).toHaveLength(2);
  expect(values[1]!.value).toEqual({ kind: "scalar", value: text("Team") });
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
  const root = await realpath(
    await mkdtemp(join(tmpdir(), "xmtp-node-attachments-")),
  );
  // The local object store is on a loopback address. 64-bit limits above
  // 2^53 must keep every bit through the option lowering and lift.
  const settings = {
    maxDownloadBytes: 2n ** 53n + 1n,
    maxPendingAgeSeconds: 2n ** 53n + 3n,
    allowPrivateNetwork: true,
  };
  const files = (name: string) =>
    clientOptions({
      storage: { location: { directory: join(root, name) }, label: "phone" },
      attachments: settings,
    });
  try {
    const sender = await Client.create(createSigner().signer, files("sender"));
    const receiver = await Client.create(
      createSigner().signer,
      files("receiver"),
    );
    expect(sender.options.attachments).toEqual(settings);
    expect(sender.options.storage.label).toBe("phone");
    // The label is the first directory below the storage root.
    expect(await sender.storage.path()).toContain(
      join(root, "sender", "phone") + sep,
    );
    expect(sender.serverConfiguration.attachments).toEqual({
      baseUrl: expect.any(String),
      maxUploadBytes: expect.any(BigInt),
      retentionSeconds: expect.any(BigInt),
    });
    expect(sender.attachments.offered).toBe(true);
    const source = (text: string) => ({
      kind: "bytes" as const,
      bytes: new TextEncoder().encode(text),
      filename: "note.txt",
      mimeType: "text/plain",
    });
    const pending = await sender.attachments.create(source("attachment bytes"));
    // A path source with no filename lowers as the path variant; Rust names
    // the file from the path.
    const otherPath = join(root, "other.bin");
    await writeFile(otherPath, "other bytes");
    const other = await sender.attachments.create({
      kind: "path",
      path: otherPath,
      filename: undefined,
      mimeType: "text/plain",
    });
    expect(other.remoteAttachment.filename).toBe("other.bin");
    const digests = (list: PendingAttachment[]) =>
      list.map((item) => item.remoteAttachment.contentDigest).sort();
    expect(digests(await sender.attachments.listPending())).toEqual(
      digests([pending, other]),
    );
    const resumed = await sender.attachments.pending(pending.remoteAttachment);
    expect(await resumed.status()).toEqual({ kind: "waiting" });
    const dm = await sender.conversations.createDm(receiver.inboxId);
    const sent = await dm.sendRemoteAttachment(pending.remoteAttachment);
    await pending.upload();
    await other.upload();
    expect(await pending.status()).toEqual({ kind: "complete" });
    expect(await resumed.status()).toEqual({ kind: "complete" });
    expect(await sender.attachments.listPending()).toEqual([]);
    // An upload whose staged data is gone fails, and its status carries the
    // same failure record.
    const broken = await sender.attachments.create(source("broken bytes"));
    const storagePath = await sender.storage.path();
    if (storagePath === undefined) throw new Error("no storage path");
    await unlink(
      join(
        dirname(storagePath),
        "attachments",
        ".staged",
        broken.remoteAttachment.contentDigest,
      ),
    );
    const uploadFailure = await broken.upload().then(
      () => undefined,
      (error: unknown) => error,
    );
    expect(uploadFailure).toBeInstanceOf(XmtpError.Attachment);
    expect(await broken.status()).toEqual({
      kind: "failed",
      value: (uploadFailure as { attachmentFailure: unknown })
        .attachmentFailure,
    });
    expect(await broken.status()).toMatchObject({
      value: { cause: "stagedUnusable" },
    });

    await receiver.conversations.syncAll(undefined);
    const message = await receiver.conversations.getMessageById(sent);
    if (message?.content.kind !== "remoteAttachment")
      throw new Error("the record did not arrive");
    const attachments = receiver.attachments;
    const events = await receiver.events({
      kinds: ["attachment.download_completed", "attachment.deleted"],
    });
    // A listener gets the same lifted public event as the reader.
    const heard: ClientEvent[] = [];
    const listener = await receiver.startListener(
      { kinds: ["attachment.deleted"] },
      (event) => {
        heard.push(event);
      },
    );
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
    // A local file lifts with a path relative to the attachment directory
    // and a Timestamp.
    const local = await attachments.listLocal();
    expect(local).toHaveLength(1);
    expect(isAbsolute(local[0]!.path)).toBe(false);
    expect(path.endsWith(sep + local[0]!.path)).toBe(true);
    expect(local[0]!.createdAt).toBeInstanceOf(Timestamp);
    await attachments.deleteLocal(message.content.value);
    expect(await attachments.listLocal()).toEqual([]);
    await expect(readFile(path)).rejects.toMatchObject({ code: "ENOENT" });
    // The download and delete events carry the attachment's reference.
    const reference = {
      attachment_key: expect.any(String),
      url: message.content.value.url,
      content_digest: message.content.value.contentDigest,
    };
    const nextEvent = async () => {
      let timer!: NodeJS.Timeout;
      const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error("no event")), 10_000);
      });
      try {
        return (await Promise.race([events.next(), timeout]))
          .value as ClientEvent;
      } finally {
        clearTimeout(timer);
      }
    };
    const completed = await nextEvent();
    const deleted = await nextEvent();
    expect(completed).toEqual({
      kind: "attachment.download_completed",
      attachment_download_completed: reference,
    });
    expect(deleted).toEqual({
      kind: "attachment.deleted",
      attachment_deleted: reference,
    });
    await vi.waitFor(() =>
      expect(heard).toEqual([
        { kind: "attachment.deleted", attachment_deleted: reference },
      ]),
    );
    await receiver.stopListener(listener);
    await events.return();
    await receiver.end();
    await sender.end();
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
