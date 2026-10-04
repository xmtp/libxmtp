import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import {
  mkdtemp,
  readFile,
  rename,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, sep } from "node:path";

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/index.ts";
import { setEventStartHookForTest } from "../../../../target/sdk-conformance/typescript-napi/runtime/client.ts";
import { serve } from "./node-support.mts";
import { heldTransfer } from "./transfer-control.mts";

const ATTACHMENT_KINDS: sdk.EventKind[] = [
  "attachment.upload_started",
  "attachment.upload_completed",
  "attachment.upload_failed",
  "attachment.download_started",
  "attachment.download_completed",
  "attachment.download_failed",
  "attachment.deleted",
];

type AttachmentEvent = Extract<
  sdk.ClientEvent,
  { readonly kind: `attachment.${string}` }
>;

function attachmentOf(event: AttachmentEvent) {
  switch (event.kind) {
    case "attachment.upload_started":
      return event.attachment_upload_started;
    case "attachment.upload_completed":
      return event.attachment_upload_completed;
    case "attachment.upload_failed":
      return event.attachment_upload_failed;
    case "attachment.download_started":
      return event.attachment_download_started;
    case "attachment.download_completed":
      return event.attachment_download_completed;
    case "attachment.download_failed":
      return event.attachment_download_failed;
    case "attachment.deleted":
      return event.attachment_deleted;
  }
}

/** A client whose files live under `root`, allowed to reach loopback storage. */
function fileOptions(
  backend: sdk.BackendOptions,
  root: string,
  attachments: sdk.AttachmentOptions = { allowPrivateNetwork: true },
): sdk.ClientOptions {
  return {
    backend,
    storage: { location: { directory: root }, singleConnection: false },
    deviceSync: false,
    attachments,
  };
}

function bytesSource(text: string): sdk.AttachmentSource {
  return {
    kind: "bytes",
    bytes: new TextEncoder().encode(text),
    filename: "note.txt",
    mimeType: "text/plain",
  };
}

function failure(
  cause: sdk.AttachmentFailureCause,
  fields: Partial<sdk.AttachmentFailure> = {},
): sdk.AttachmentFailure {
  return {
    cause,
    credentialKind: undefined,
    retryable: false,
    missingScope: false,
    httpStatus: undefined,
    ...fields,
  };
}

async function thrownError(
  action: Promise<unknown>,
): Promise<InstanceType<typeof sdk.XmtpError.Attachment>> {
  try {
    await action;
  } catch (error) {
    assert.ok(
      error instanceof sdk.XmtpError.Attachment,
      `expected an attachment error, got ${String(error)}`,
    );
    assert.equal(error.details.code, "Attachment");
    return error;
  }
  throw new Error("expected an attachment error, got success");
}

async function thrownFailure(
  action: Promise<unknown>,
): Promise<sdk.AttachmentFailure> {
  return (await thrownError(action)).attachmentFailure;
}

function isClientClosed(error: unknown): boolean {
  return error instanceof sdk.XmtpError.ClientClosed;
}

function attachmentFilter(
  kinds: sdk.EventKind[] = ATTACHMENT_KINDS,
): sdk.EventFilter {
  return {
    kinds: [...kinds, "conversation.joined"],
    referencesOwnMessages: false,
  };
}

async function within<T>(promise: Promise<T>, what: string): Promise<T> {
  let timer!: NodeJS.Timeout;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} timed out`)), 10_000);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/** Read attachment events up to the group this creates, which marks the end. */
async function drain(
  client: sdk.Client,
  stream: sdk.EventStream,
): Promise<AttachmentEvent[]> {
  const marker = (await client.conversations.createGroup([])).id;
  const events: AttachmentEvent[] = [];
  for (;;) {
    const next = await within(stream.next(), "attachment events");
    assert.equal(next.done, false, "the event stream ended");
    const event = next.value as sdk.ClientEvent;
    if (event.kind === "conversation.joined") {
      if (event.conversation_joined.conversationId === marker) return events;
      continue;
    }
    assert.ok(
      event.kind.startsWith("attachment."),
      `unexpected ${event.kind} event`,
    );
    events.push(event as AttachmentEvent);
  }
}

function attachmentsDir(databasePath: string | undefined): string {
  assert.ok(databasePath);
  return join(dirname(databasePath), "attachments");
}

/** Storage and server configuration fields, including 64-bit values. */
export async function attachmentSettings(
  backend: sdk.BackendOptions,
): Promise<void> {
  const offered = {
    baseUrl:
      process.env.XMTP_S3_BASE_URL ?? "http://127.0.0.1:9067/attachments",
    maxUploadBytes: 104_857_600n,
    retentionSeconds: 0n,
  };
  assert.deepEqual(
    (await sdk.fetchServerConfiguration(backend)).attachments,
    offered,
  );
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const large = 2n ** 53n + 1n;
  const settings = {
    maxDownloadBytes: large,
    maxPendingAgeSeconds: large + 2n,
    allowPrivateNetwork: true,
  };
  const client = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions(backend, root, settings),
  );
  assert.deepEqual(client.serverConfiguration.attachments, offered);
  assert.deepEqual(client.options.attachments, settings);
  assert.equal(client.attachments.offered, true);
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node attachments: configuration and 64-bit options");
}

/** Create, send, upload, reopen, resume, and download on another client. */
export async function attachmentFlow(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const signer = await sdk.generateLocalSigner();
  const senderOptions = fileOptions(backend, join(root, "sender"));
  const sender = await sdk.Client.create(signer, senderOptions);
  const receiver = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions(backend, join(root, "receiver")),
  );
  const attachments = sender.attachments;
  const events = await sender.events(attachmentFilter());
  // The receiver's reader sees none of the sender's events.
  const receiverEvents = await receiver.events(attachmentFilter());

  const content = "attachment bytes";
  const fromBytes = await attachments.create(bytesSource(content));
  const remote = fromBytes.remoteAttachment;
  assert.deepEqual(await fromBytes.status(), { kind: "waiting" });
  assert.equal(await readFile(await fromBytes.localPath(), "utf8"), content);
  assert.equal(
    await attachments.localPath(remote),
    await fromBytes.localPath(),
  );
  // The SDK copies a path source at create, so moving it changes nothing.
  const source = join(root, "photo.bin");
  await writeFile(source, "path bytes");
  const fromPath = await attachments.create({
    kind: "path",
    path: source,
    filename: undefined,
    mimeType: "application/octet-stream",
  });
  await rename(source, join(root, "moved.bin"));
  const pathRemote = fromPath.remoteAttachment;
  assert.equal(
    await readFile(await fromPath.localPath(), "utf8"),
    "path bytes",
  );
  assert.deepEqual(await drain(sender, events), []);

  // The record is complete before any upload, so the app sends it first.
  const dm = await sender.conversations.createDm(receiver.inboxId);
  const sent = await dm.sendRemoteAttachment(remote);
  // Concurrent uploads of one attachment share one transfer.
  await Promise.all([fromBytes.upload(), fromBytes.upload()]);
  assert.deepEqual(await fromBytes.status(), { kind: "complete" });
  const uploaded = await drain(sender, events);
  assert.deepEqual(
    uploaded.map((event) => event.kind),
    ["attachment.upload_started", "attachment.upload_completed"],
    "expected one shared upload",
  );
  assert.deepEqual(attachmentOf(uploaded[0]!), attachmentOf(uploaded[1]!));
  assert.equal(attachmentOf(uploaded[0]!).url, remote.url);
  assert.equal(attachmentOf(uploaded[0]!).contentDigest, remote.contentDigest);
  assert.deepEqual(await drain(receiver, receiverEvents), []);
  await events.return();
  await sender.end();
  // Held values stay readable after end; calls fail closed.
  assert.equal(attachments.offered, true);
  assert.deepEqual(fromBytes.remoteAttachment, remote);
  await assert.rejects(fromPath.status(), isClientClosed);
  await assert.rejects(attachments.listPending(), isClientClosed);

  // A reopened client lists the upload it did not finish and resumes it.
  const reopened = await sdk.Client.build(
    await signer.identity(),
    senderOptions,
  );
  const resuming = reopened.attachments;
  const listed = await resuming.listPending();
  assert.equal(listed.length, 1, "expected one pending upload");
  assert.equal(
    listed[0]!.remoteAttachment.contentDigest,
    pathRemote.contentDigest,
  );
  assert.deepEqual(await (await resuming.pending(remote)).status(), {
    kind: "complete",
  });
  const resumed = await resuming.pending(pathRemote);
  assert.deepEqual(await resumed.status(), { kind: "waiting" });
  await resumed.upload();
  assert.deepEqual(await resumed.status(), { kind: "complete" });
  assert.deepEqual(await resuming.listPending(), []);
  assert.deepEqual(
    await thrownFailure(
      resuming.pending({ ...pathRemote, contentDigest: "00".repeat(32) }),
    ),
    failure("stagedUnusable"),
  );

  // The receiver derives the path of the record it was sent without a request.
  await receiver.conversations.syncAll(undefined);
  const message = await receiver.conversations.getMessageById(sent);
  if (message?.content.kind !== "remoteAttachment")
    throw new Error(
      "the sent attachment did not arrive as a remote attachment",
    );
  const received = message.content.value;
  const receiving = receiver.attachments;
  const directory = attachmentsDir(await receiver.storage.path());
  const probe = await serve(200, new Uint8Array());
  const derived = await receiving.localPath({ ...received, url: probe.url });
  assert.equal(probe.requests(), 0, "path derivation sent a request");
  assert.ok(derived.startsWith(`${directory}${sep}`));
  await probe.close();
  const expectedPath = await receiving.localPath(received);
  assert.equal(existsSync(expectedPath), false);

  // Download events arrive in order; a filtered reader sees only its kind.
  const deletedOnly = await receiver.events(
    attachmentFilter(["attachment.deleted"]),
  );
  const downloaded = await receiving.download(received);
  assert.deepEqual(downloaded, {
    path: expectedPath,
    mimeType: "text/plain",
    filename: "note.txt",
  });
  assert.equal(await readFile(downloaded.path, "utf8"), content);
  const pathDownload = await receiving.download(pathRemote);
  assert.equal(await readFile(pathDownload.path, "utf8"), "path bytes");
  const local = await receiving.listLocal();
  assert.ok(local.every((file) => file.createdAt instanceof sdk.Timestamp));
  assert.deepEqual(
    local.map((file) => file.path).sort(),
    [
      relative(directory, downloaded.path),
      relative(directory, pathDownload.path),
    ].sort(),
  );
  await receiving.deleteLocal(received);
  assert.equal((await receiving.listLocal()).length, 1);
  assert.equal(existsSync(downloaded.path), false);
  const downloads = await drain(receiver, receiverEvents);
  assert.deepEqual(
    downloads.map((event) => [event.kind, attachmentOf(event).contentDigest]),
    [
      ["attachment.download_started", received.contentDigest],
      ["attachment.download_completed", received.contentDigest],
      ["attachment.download_started", pathRemote.contentDigest],
      ["attachment.download_completed", pathRemote.contentDigest],
      ["attachment.deleted", received.contentDigest],
    ],
  );
  assert.equal(attachmentOf(downloads[0]!).url, received.url);
  assert.notEqual(
    attachmentOf(downloads[0]!).attachmentKey,
    attachmentOf(downloads[2]!).attachmentKey,
  );
  assert.deepEqual(attachmentOf(downloads[4]!), attachmentOf(downloads[0]!));
  assert.deepEqual(await drain(receiver, deletedOnly), [downloads[4]]);
  await deletedOnly.return();
  await receiverEvents.return();
  await reopened.end();
  await receiver.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node attachments: upload, reopen, resume, download, and delete");
}

/** Failed uploads and downloads carry one record in errors and status. */
export async function attachmentFailures(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const client = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions(backend, root),
  );
  const attachments = client.attachments;
  const events = await client.events(attachmentFilter());
  const staged = join(attachmentsDir(await client.storage.path()), ".staged");

  // Missing staged data fails the upload before any request.
  const pending = await attachments.create(bytesSource("staged"));
  const remote = pending.remoteAttachment;
  const ciphertext = await readFile(join(staged, remote.contentDigest));
  await unlink(join(staged, remote.contentDigest));
  const other = await attachments.create(bytesSource("other"));
  const otherRemote = other.remoteAttachment;
  const otherCiphertext = await readFile(
    join(staged, otherRemote.contentDigest),
  );
  const thrown = await thrownFailure(pending.upload());
  assert.deepEqual(thrown, failure("stagedUnusable"));
  assert.deepEqual(await pending.status(), { kind: "failed", value: thrown });
  const failed = await drain(client, events);
  assert.deepEqual(
    failed.map((event) => event.kind),
    ["attachment.upload_started", "attachment.upload_failed"],
  );
  assert.deepEqual(attachmentOf(failed[1]!), {
    ...attachmentOf(failed[0]!),
    cause: "stagedUnusable",
  });
  assert.equal(attachmentOf(failed[1]!).contentDigest, remote.contentDigest);
  // A source the SDK cannot read fails create.
  assert.equal(
    (
      await thrownFailure(
        attachments.create({
          kind: "path",
          path: join(root, "missing.bin"),
          filename: undefined,
          mimeType: "application/octet-stream",
        }),
      )
    ).cause,
    "sourceUnreadable",
  );
  await events.return();

  // The creating client holds the plaintext, so another client downloads.
  const downloader = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions(backend, join(root, "downloader")),
  );
  const downloads = downloader.attachments;
  const downloadEvents = await downloader.events(attachmentFilter());
  const unavailable = await serve(503, new Uint8Array());
  const error = await thrownError(
    downloads.download({ ...remote, url: unavailable.url }),
  );
  assert.deepEqual(
    error.attachmentFailure,
    failure("httpStatus", { httpStatus: 503 }),
  );
  assert.equal(error.details.category, "network");
  assert.equal(error.details.retryable, true);
  assert.equal(unavailable.requests(), 1, "the SDK does not retry");
  await unavailable.close();

  // Another attachment's object decrypts and decodes, so only its digest
  // differs from the record.
  const substituted = await serve(200, otherCiphertext);
  assert.equal(
    (
      await thrownFailure(
        downloads.download({
          ...otherRemote,
          url: substituted.url,
          contentDigest: remote.contentDigest,
        }),
      )
    ).cause,
    "digestMismatch",
  );
  await substituted.close();
  // A changed tag byte with a matching digest fails only the decryption.
  const tamperedBytes = Uint8Array.from(ciphertext);
  tamperedBytes[tamperedBytes.length - 1]! ^= 1;
  const tampered = await serve(200, tamperedBytes);
  assert.equal(
    (
      await thrownFailure(
        downloads.download({
          ...remote,
          url: tampered.url,
          contentDigest: createHash("sha256")
            .update(tamperedBytes)
            .digest("hex"),
        }),
      )
    ).cause,
    "decryptionFailed",
  );
  assert.equal(
    (
      await thrownFailure(
        downloads.download({
          ...remote,
          url: tampered.url,
          secret: new Uint8Array([7, 7, 7]),
        }),
      )
    ).cause,
    "malformed",
  );
  await tampered.close();
  const downloadFailures = await drain(downloader, downloadEvents);
  assert.deepEqual(
    downloadFailures.flatMap((event) =>
      event.kind === "attachment.download_failed"
        ? [event.attachment_download_failed.cause]
        : [],
    ),
    ["httpStatus", "digestMismatch", "decryptionFailed"],
  );
  await downloadEvents.return();
  await downloader.end();
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node attachments: real failures carry one record");
}

// Every transport discriminant and optional field. Rust owns the full policy table.
const FAILURE_TABLE: sdk.AttachmentFailure[] = [
  failure("notOffered"),
  failure("tooLarge"),
  failure("sourceUnreadable"),
  failure("localStorage"),
  failure("stagedUnusable"),
  failure("connectionBlocked"),
  failure("credential", {
    credentialKind: "credentialRejected",
    missingScope: true,
  }),
  failure("credential", {
    credentialKind: "callbackFailed",
    retryable: true,
  }),
  failure("credential", { credentialKind: "exhausted" }),
  failure("credential", { credentialKind: "missingCredential" }),
  failure("backendRejected"),
  failure("backendUnavailable"),
  failure("targetRejected", { httpStatus: 403 }),
  failure("network"),
  failure("insecureUrl"),
  failure("blockedAddress"),
  failure("tooManyRedirects"),
  failure("notFound", { httpStatus: 404 }),
  failure("httpStatus", { httpStatus: 408 }),
  failure("httpStatus", { httpStatus: 429 }),
  failure("httpStatus", { httpStatus: 503 }),
  failure("httpStatus", { httpStatus: 403 }),
  failure("malformed"),
  failure("digestMismatch"),
  failure("decryptionFailed"),
  failure("notAnAttachment"),
  failure("deleted"),
];

/** Every cause and credential kind, thrown and recorded, and no resend. */
export async function attachmentRecords(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const held = await heldTransfer(process.env.SDK_FIXTURE_URL!, true);
  const client = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions({ ...backend, url: held.backend }, root),
  );
  const attachments = client.attachments;
  for (const [index, recorded] of FAILURE_TABLE.entries()) {
    const error = await thrownError(
      sdk.sdkConformanceAttachmentError(recorded),
    );
    assert.deepEqual(error.attachmentFailure, recorded);
    if (recorded.cause === "credential") {
      assert.equal(error.details.category, "callback", recorded.cause);
      assert.equal(error.details.retryable, recorded.retryable, recorded.cause);
    }
    const pending = await attachments.create(bytesSource(`record ${index}`));
    await pending.sdkConformanceFail(recorded);
    assert.deepEqual(await pending.status(), {
      kind: "failed",
      value: recorded,
    });
  }
  const causes = new Set(FAILURE_TABLE.map((recorded) => recorded.cause));
  assert.equal(causes.size, 21);
  const kinds = new Set(
    FAILURE_TABLE.flatMap((recorded) => recorded.credentialKind ?? []),
  );
  assert.equal(kinds.size, 4);

  // A terminal backend rejection is not sent again.
  await held.command("release");
  const events = await client.events(attachmentFilter());
  const rejected = await attachments.create(bytesSource("rejected"));
  await rejected.sdkConformanceFail(failure("backendRejected"));
  for (let attempt = 0; attempt < 2; attempt += 1)
    assert.deepEqual(
      await thrownFailure(rejected.upload()),
      failure("backendRejected"),
    );
  assert.deepEqual(await rejected.status(), {
    kind: "failed",
    value: failure("backendRejected"),
  });
  assert.deepEqual(await drain(client, events), []);
  await events.return();
  assert.deepEqual(
    await (await held.command("counts")).json(),
    { puts: 0, grants: 0, gets: 0 },
    "terminal rejection sent a request",
  );
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log(
    "Node attachments: every cause and credential kind in both forms",
  );
}

/** End waits for an operation in flight; later calls fail closed. */
export async function attachmentEnd(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const signer = await sdk.generateLocalSigner();
  const held = await heldTransfer(process.env.SDK_FIXTURE_URL!, true);
  const options = fileOptions({ ...backend, url: held.backend }, root);
  const client = await sdk.Client.create(signer, options);
  let reopened: sdk.Client | undefined;
  try {
    const attachments = client.attachments;
    const small = await attachments.create(bytesSource("small"));
    const events = await client.events(
      attachmentFilter(["attachment.upload_started"]),
    );
    const large = await attachments.create(bytesSource("held upload"));
    const upload = large.upload();
    await within(held.command("entered"), "held PUT");
    const started = await within(events.next(), "upload start");
    assert.equal(started.value?.kind, "attachment.upload_started");
    let ended = false;
    const ending = client.end().then(() => {
      ended = true;
    });
    // The ended subscription proves Rust has entered client shutdown.
    assert.equal((await within(events.next(), "event end")).done, true);
    assert.equal(ended, false, "end released storage while PUT was held");
    assert.deepEqual(await (await held.command("counts")).json(), {
      puts: 1,
      grants: 1,
      gets: 0,
    });
    await held.command("release");
    await within(ending, "end after release");
    await within(upload, "upload across end");
    // Held values stay readable; calls fail closed.
    assert.equal(attachments.offered, true);
    const remote = large.remoteAttachment;
    const closedCalls: Array<() => Promise<unknown>> = [
      () => attachments.create(bytesSource("late")),
      () => attachments.localPath(remote),
      () => attachments.listLocal(),
      () => attachments.download(remote),
      () => attachments.pending(remote),
      () => attachments.listPending(),
      () => attachments.deleteLocal(remote),
      () => large.localPath(),
      () => large.status(),
      () => large.upload(),
    ];
    for (const call of closedCalls)
      await assert.rejects(within(call(), "closed call"), isClientClosed);

    // The held wrappers do not keep the ended database open.
    reopened = await sdk.Client.build(await signer.identity(), options);
    const resumed = reopened.attachments;
    assert.deepEqual(await (await resumed.pending(remote)).status(), {
      kind: "complete",
    });
    assert.deepEqual(await (await held.command("counts")).json(), {
      puts: 1,
      grants: 1,
      gets: 0,
    });
    // A listener sees a deletion until it stops.
    const first = Promise.withResolvers<void>();
    let deletions = 0;
    const listener = await reopened.startListener(
      { kinds: ["attachment.deleted"], referencesOwnMessages: false },
      () => {
        deletions += 1;
        first.resolve();
      },
    );
    const deleted = await reopened.events(
      attachmentFilter(["attachment.deleted"]),
    );
    await resumed.deleteLocal(remote);
    await within(first.promise, "first deletion callback");
    assert.equal(deletions, 1);
    const entered = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    const finished = Promise.withResolvers<void>();
    setEventStartHookForTest(
      async () => {
        entered.resolve();
        await release.promise;
      },
      async () => {
        finished.resolve();
      },
    );
    try {
      await resumed.deleteLocal(small.remoteAttachment);
      await within(entered.promise, "held deletion callback");
      await reopened.stopListener(listener);
      release.resolve();
      await within(finished.promise, "stopped callback dispatch");
      assert.equal(deletions, 1, "a stopped listener saw a deletion");
    } finally {
      release.resolve();
      setEventStartHookForTest();
    }
    assert.equal(existsSync(await resumed.localPath(remote)), false);
    // The reader has both deletions, so a live listener had its turn.
    for (const expected of [remote, small.remoteAttachment]) {
      const next = (await within(deleted.next(), "deletion")).value;
      if (next?.kind !== "attachment.deleted")
        throw new Error("not a deletion");
      assert.equal(next.attachment_deleted.url, expected.url);
    }
    assert.equal(deletions, 1, "a stopped listener saw a deletion");
    await deleted.return();
    await reopened.end();
    await rm(root, { recursive: true, force: true });
    console.log("Node attachments: end waits for an upload; calls fail closed");
  } finally {
    await held.command("release");
    await reopened?.end();
    await client.end();
    await rm(root, { recursive: true, force: true });
  }
}
