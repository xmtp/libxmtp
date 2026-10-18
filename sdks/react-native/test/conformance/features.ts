import {
  Client,
  XmtpError,
  decryptFile,
  encryptFile,
  metadataFieldRef,
  type ClientOptions,
  type ContentCodec,
  type Signer,
} from "@xmtp/react-native-sdk";
import { NativeModules } from "react-native";

import { assert, bytesEqual, equal } from "./assertions";
import { contentVectorJourney } from "./content-vectors";
import { identityJourney } from "./identity";
import type { JourneyControl } from "./journey";

export const fixtureCodec: ContentCodec<Uint8Array> = {
  type: {
    authorityId: "xmtp.org",
    typeId: "rn-conformance",
    versionMajor: 1,
    versionMinor: 0,
  },
  encode: (content) => ({ type: fixtureCodec.type, content }),
  decode: (envelope) => envelope.content,
  shouldPush: () => false,
};

/** Representative native feature calls; Rust tests retain protocol/crypto coverage. */
export async function featureJourney(
  client: Client,
  signer: Signer,
  options: ClientOptions,
  peerInboxId: string,
  control: JourneyControl,
): Promise<readonly string[]> {
  const evidence: string[] = [
    ...contentVectorJourney(),
    ...(await identityJourney(options)),
  ];
  const group = await client.conversations.createGroup([], {
    name: "RN initial",
  });
  await group.addMembers([peerInboxId]);
  await group.addAdmin(peerInboxId);
  await group.updatePermission("addMember", "admin", undefined);
  const field = metadataFieldRef("groupName");
  await group.updateMetadataField(field, {
    kind: "replace",
    value: { kind: "string", value: "RN metadata" },
  });
  const textId = await group.sendText("RN feature history");
  const customId = await group.send(
    fixtureCodec,
    new Uint8Array([91, 0, 128, 255, 92]).subarray(1, 4),
  );
  const custom = await client.conversations.getMessageById(customId);
  assert(
    custom?.content.kind === "custom",
    "registered custom codec must decode through native message projection",
  );
  assert(
    custom.content.value instanceof Uint8Array,
    "custom codec must return bytes",
  );
  bytesEqual(
    custom.content.value,
    [0, 128, 255],
    "custom codec must retain sliced bytes",
  );
  await control.call("verify-group", {
    conversationId: group.id,
    textId,
    customId,
    senderInboxId: client.inboxId,
  });
  evidence.push(
    "Node sibling verifies native members, admin, permissions, metadata, standard and custom content",
  );
  await group.removeAdmin(peerInboxId);
  equal(
    await group.isAdmin(peerInboxId),
    false,
    "admin removal must be visible through the public API",
  );

  const entity = { kind: "conversation" as const, conversationId: group.id };
  await client.preferences.setConsentStates([{ entity, state: "denied" }]);
  equal(
    (await group.state()).common.consentState,
    "denied",
    "preferences must update conversation consent",
  );
  await group.updateConsentState("allowed");
  equal(
    await client.preferences.consentState(entity),
    "allowed",
    "conversation consent must update preferences",
  );
  const catchup = await client.catchUpToLive(undefined);
  equal(
    typeof catchup.messages,
    "bigint",
    "catch-up counters must retain bigint types",
  );
  assert(catchup.completed, "native catch-up must reach live state");
  evidence.push("consent and catch-up public roundtrip");

  const storagePath = client.storagePath;
  assert(storagePath, "feature journey requires native private storage");
  const archivePath = `${storagePath}.history.xmtp`;
  const ciphertextPath = `${storagePath}.history.encrypted`;
  const restoredPath = `${storagePath}.history.restored.xmtp`;
  const archiveKey = new Uint8Array(32).fill(29);
  await client.archives.exportToFile(archivePath, archiveKey, undefined);
  const keys = await encryptFile(archivePath, ciphertextPath);
  await decryptFile(ciphertextPath, restoredPath, keys);
  assert(
    (await client.archives.metadataFromFile(restoredPath, archiveKey)).elements
      .length > 0,
    "native file decrypt must produce a readable archive",
  );
  let imported: Client | undefined;
  try {
    imported = await Client.create(signer, {
      ...options,
      storage: {
        ...options.storage,
        location: {
          dbPath: `${storagePath}.import.db3`,
          attachmentsDir: `${storagePath}.import.attachments`,
        },
      },
    });
    await imported.archives.importFromFile(restoredPath, archiveKey);
    equal(
      (await imported.conversations.getMessageById(textId))?.id,
      textId,
      "native archive import must restore independent message id",
    );
    const restored = await imported.conversations.getMessageById(customId);
    assert(
      restored?.content.kind === "custom" &&
        restored.content.value instanceof Uint8Array,
      "archive import must restore custom content",
    );
    bytesEqual(
      restored.content.value,
      [0, 128, 255],
      "archive import must retain custom payload",
    );
    equal(
      await imported.preferences.consentState(entity),
      "allowed",
      "archive import must restore consent",
    );
  } finally {
    await imported?.end();
  }
  evidence.push(
    "native file encrypt/decrypt and archive import retain message ids, bytes and consent",
  );

  assert(
    client.attachments.offered,
    "local backend must offer integrated attachments",
  );
  const pending = await client.attachments.create({
    kind: "path",
    path: restoredPath,
    filename: "history.xmtp",
    mimeType: "application/octet-stream",
  });
  const dm = await client.conversations.createDm(peerInboxId);
  const attachmentId = await dm.sendRemoteAttachment(pending.remoteAttachment);
  await pending.upload();
  equal(
    (await pending.status()).kind,
    "complete",
    "native path attachment must upload",
  );
  await control.call("verify-attachment", {
    messageId: attachmentId,
    archiveKey: Array.from(archiveKey),
  });
  const files = NativeModules.XmtpMigrationFixture as {
    readFile(path: string): Promise<number[]>;
  };
  assert(files?.readFile, "installed native file oracle must exist");
  const downloadDirectory = `${storagePath}.download.attachments`;
  const downloader = await Client.create(signer, {
    ...options,
    storage: {
      location: {
        dbPath: `${storagePath}.download.db3`,
        attachmentsDir: downloadDirectory,
      },
    },
  });
  try {
    const downloaded = await downloader.attachments.download(
      pending.remoteAttachment,
    );
    assert(
      downloaded.path.startsWith(`${downloadDirectory}/`),
      "downloaded attachment belongs to the requested native directory",
    );
    const relativeDownload = downloaded.path.slice(
      downloadDirectory.length + 1,
    );
    equal(
      downloaded.filename,
      "history.xmtp",
      "RN download preserves filename",
    );
    assert(
      downloaded.path.startsWith(
        storagePath.slice(0, storagePath.lastIndexOf("/") + 1),
      ),
      "RN attachment download stays in private storage",
    );
    bytesEqual(
      new Uint8Array(await files.readFile(downloaded.path)),
      await files.readFile(restoredPath),
      "RN download contains exact known archive bytes",
    );
    assert(
      (await downloader.archives.metadataFromFile(downloaded.path, archiveKey))
        .elements.length > 0,
      "RN downloaded file is a readable native archive",
    );
    assert(
      (await downloader.attachments.listLocal()).some(
        (file) => file.path === relativeDownload,
      ),
      "RN local attachment listing includes downloaded archive",
    );
    await downloader.attachments.deleteLocal(pending.remoteAttachment);
    assert(
      !(await downloader.attachments.listLocal()).some(
        (file) => file.path === relativeDownload,
      ),
      "RN attachment deletion removes local listing",
    );
    let deleted = false;
    try {
      await files.readFile(downloaded.path);
    } catch {
      deleted = true;
    }
    assert(deleted, "RN attachment deletion removes the native file");
  } finally {
    await downloader.end();
  }
  evidence.push(
    "RN and independent Node sibling download native archive; RN verifies exact private bytes, readable archive, listing and deletion",
  );

  equal(
    client.notificationState().kind,
    "disabled",
    "notifications start disabled",
  );
  equal(
    (
      await client.enableNotifications({
        channel: {
          kind: "http",
          url: "https://example.com/xmtp-notification-test",
          signingKey: new Uint8Array(32).fill(17),
        },
      })
    ).kind,
    "enabled",
    "HTTP notification registration must succeed",
  );
  await group.setNotifications("disabled");
  equal(
    (await group.state()).common.notificationsEnabled,
    false,
    "conversation notification override must persist",
  );
  await group.setNotifications("default");
  equal(
    (await group.state()).common.notificationsEnabled,
    true,
    "default override must restore notification settings",
  );
  await client.disableNotifications();
  for (const channel of [
    { kind: "apns" as const, token: "a".repeat(64) },
    { kind: "fcm" as const, token: "rn-fixture-token" },
  ]) {
    let failure: unknown;
    try {
      await client.enableNotifications({ channel });
    } catch (error) {
      failure = error;
    }
    assert(
      failure instanceof XmtpError.ChannelNotConfigured,
      "unconfigured native push channel must preserve its typed backend error",
    );
    equal(
      client.notificationState().kind,
      "failed",
      "native push failure must update notification state",
    );
    await client.disableNotifications();
  }
  evidence.push(
    "HTTP push and overrides roundtrip; APNS/FCM preserve typed unconfigured-channel failures",
  );
  return evidence;
}
