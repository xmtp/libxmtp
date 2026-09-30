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

import * as sdk from "../../../../target/sdk-conformance/typescript-napi/public-api.gen.ts";
import { serve } from "./node-support.mts";

const ATTACHMENT_KINDS: sdk.EventKind[] = [
  "attachmentUploadStarted",
  "attachmentUploadCompleted",
  "attachmentUploadFailed",
  "attachmentDownloadStarted",
  "attachmentDownloadCompleted",
  "attachmentDownloadFailed",
  "attachmentDeleted",
];

type AttachmentEvent = Extract<
  sdk.ClientEvent,
  { readonly attachment: unknown }
>;

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
    kinds: [...kinds, "conversationJoined"],
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
    if (event.kind === "conversationJoined") {
      if (event.conversationId === marker) return events;
      continue;
    }
    assert.ok("attachment" in event, `unexpected ${event.kind} event`);
    events.push(event);
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
  // verifies: CONF-061, CONF-062
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
  // verifies: ATCH-009
  assert.equal(client.attachments.offered, true);
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node CONF-061: attachment configuration and 64-bit options");
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
    ["attachmentUploadStarted", "attachmentUploadCompleted"],
    "expected one shared upload",
  );
  assert.deepEqual(uploaded[0]!.attachment, uploaded[1]!.attachment);
  assert.equal(uploaded[0]!.attachment.url, remote.url);
  assert.equal(uploaded[0]!.attachment.contentDigest, remote.contentDigest);
  assert.deepEqual(await drain(receiver, receiverEvents), []);
  await events.return();
  await sender.end();
  // Held values stay readable after end; calls fail closed.
  assert.equal(attachments.offered, true);
  assert.deepEqual(fromBytes.remoteAttachment, remote);
  await assert.rejects(fromPath.status(), isClientClosed);
  await assert.rejects(attachments.listPending(), isClientClosed);

  // verifies: ATCH-038, ATCH-067
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

  // verifies: ATCH-044
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

  // verifies: ATCH-062, EVENT-015, EVENT-020
  const deletedOnly = await receiver.events(
    attachmentFilter(["attachmentDeleted"]),
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
    downloads.map((event) => [event.kind, event.attachment.contentDigest]),
    [
      ["attachmentDownloadStarted", received.contentDigest],
      ["attachmentDownloadCompleted", received.contentDigest],
      ["attachmentDownloadStarted", pathRemote.contentDigest],
      ["attachmentDownloadCompleted", pathRemote.contentDigest],
      ["attachmentDeleted", received.contentDigest],
    ],
  );
  assert.equal(downloads[0]!.attachment.url, received.url);
  assert.notEqual(
    downloads[0]!.attachment.attachmentKey,
    downloads[2]!.attachment.attachmentKey,
  );
  assert.deepEqual(downloads[4]!.attachment, downloads[0]!.attachment);
  assert.deepEqual(await drain(receiver, deletedOnly), [downloads[4]]);
  await deletedOnly.return();
  await receiverEvents.return();
  await reopened.end();
  await receiver.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node ATCH-038: upload, reopen, resume, download, and delete");
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

  // verifies: ATCH-060
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
    ["attachmentUploadStarted", "attachmentUploadFailed"],
  );
  assert.deepEqual(failed[1]!.attachment, {
    ...failed[0]!.attachment,
    cause: "stagedUnusable",
  });
  assert.equal(failed[1]!.attachment.contentDigest, remote.contentDigest);
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

  // verifies: ATCH-079
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
      event.kind === "attachmentDownloadFailed" ? [event.attachment.cause] : [],
    ),
    ["httpStatus", "digestMismatch", "decryptionFailed"],
  );
  await downloadEvents.return();
  await downloader.end();
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node ATCH-060: real failures carry one record");
}

// Each cause with the error category and retry the ATCH table gives it.
const FAILURE_TABLE: [sdk.AttachmentFailure, sdk.ErrorCategory, boolean][] = [
  [failure("notOffered"), "configuration", false],
  [failure("tooLarge"), "input", false],
  [failure("sourceUnreadable"), "input", false],
  [failure("localStorage"), "storage", true],
  [failure("stagedUnusable"), "storage", false],
  [failure("connectionBlocked"), "configuration", false],
  [
    failure("credential", {
      credentialKind: "credentialRejected",
      missingScope: true,
    }),
    "callback",
    false,
  ],
  [
    failure("credential", {
      credentialKind: "callbackFailed",
      retryable: true,
    }),
    "callback",
    true,
  ],
  [failure("credential", { credentialKind: "exhausted" }), "callback", false],
  [
    failure("credential", { credentialKind: "missingCredential" }),
    "callback",
    false,
  ],
  [failure("backendRejected"), "network", false],
  [failure("backendUnavailable"), "network", true],
  [failure("targetRejected", { httpStatus: 403 }), "network", true],
  [failure("network"), "network", true],
  [failure("insecureUrl"), "input", false],
  [failure("blockedAddress"), "network", false],
  [failure("tooManyRedirects"), "network", false],
  [failure("notFound", { httpStatus: 404 }), "network", true],
  [failure("httpStatus", { httpStatus: 408 }), "network", true],
  [failure("httpStatus", { httpStatus: 429 }), "network", true],
  [failure("httpStatus", { httpStatus: 503 }), "network", true],
  [failure("httpStatus", { httpStatus: 403 }), "network", false],
  [failure("malformed"), "input", false],
  [failure("digestMismatch"), "input", false],
  [failure("decryptionFailed"), "input", false],
  [failure("notAnAttachment"), "input", false],
  [failure("deleted"), "storage", true],
];

/** Every cause and credential kind, thrown and recorded, and no resend. */
export async function attachmentRecords(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const client = await sdk.Client.create(
    await sdk.generateLocalSigner(),
    fileOptions(backend, root),
  );
  const attachments = client.attachments;
  // verifies: ATCH-060, ATCH-061, ATCH-079
  for (const [
    index,
    [recorded, category, retryable],
  ] of FAILURE_TABLE.entries()) {
    const error = await thrownError(
      sdk.sdkConformanceAttachmentError(recorded),
    );
    assert.deepEqual(error.attachmentFailure, recorded);
    assert.equal(error.details.category, category, recorded.cause);
    assert.equal(error.details.retryable, retryable, recorded.cause);
    const pending = await attachments.create(bytesSource(`record ${index}`));
    await pending.sdkConformanceFail(recorded);
    assert.deepEqual(await pending.status(), {
      kind: "failed",
      value: recorded,
    });
  }
  const causes = new Set(FAILURE_TABLE.map(([recorded]) => recorded.cause));
  assert.equal(causes.size, 21);
  const kinds = new Set(
    FAILURE_TABLE.flatMap(([recorded]) => recorded.credentialKind ?? []),
  );
  assert.equal(kinds.size, 4);

  // A terminal backend rejection is not sent again.
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
  await client.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node ATCH-061: every cause and credential kind in both forms");
}

/** End waits for an operation in flight; later calls fail closed. */
export async function attachmentEnd(
  backend: sdk.BackendOptions,
): Promise<void> {
  const root = realpathSync(await mkdtemp(join(tmpdir(), "xmtp-sdk-atch-")));
  const signer = await sdk.generateLocalSigner();
  const options = fileOptions(backend, root);
  const client = await sdk.Client.create(signer, options);
  const attachments = client.attachments;
  const small = await attachments.create(bytesSource("small"));
  const events = await client.events(
    attachmentFilter(["attachmentUploadStarted"]),
  );
  const large = await attachments.create({
    kind: "bytes",
    bytes: new Uint8Array(32 * 1024 * 1024).fill(1),
    filename: undefined,
    mimeType: "application/octet-stream",
  });
  const upload = large.upload();
  const started = await within(events.next(), "upload start");
  assert.equal(started.value?.kind, "attachmentUploadStarted");
  await client.end();
  // The upload held the client, so end let it finish.
  await within(upload, "upload across end");
  assert.equal((await within(events.next(), "event end")).done, true);
  // Held values stay readable; calls fail closed.
  assert.equal(attachments.offered, true);
  const remote = large.remoteAttachment;
  const closedCalls: Array<() => Promise<unknown>> = [
    () => attachments.create(bytesSource("late")),
    () => attachments.localPath(remote),
    () => attachments.listLocal(),
    () => attachments.download(remote),
    () => large.localPath(),
    () => large.status(),
    () => large.upload(),
  ];
  for (const call of closedCalls)
    await assert.rejects(within(call(), "closed call"), isClientClosed);

  // The held wrappers do not keep the ended database open.
  const reopened = await sdk.Client.build(await signer.identity(), options);
  const resumed = reopened.attachments;
  assert.deepEqual(await (await resumed.pending(remote)).status(), {
    kind: "complete",
  });
  // A listener sees a deletion until it stops.
  let deletions = 0;
  const listener = await reopened.startListener(
    { kinds: ["attachmentDeleted"], referencesOwnMessages: false },
    () => {
      deletions += 1;
    },
  );
  const deleted = await reopened.events(
    attachmentFilter(["attachmentDeleted"]),
  );
  await resumed.deleteLocal(remote);
  for (let attempt = 0; attempt < 100 && deletions === 0; attempt += 1)
    await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(deletions, 1);
  await reopened.stopListener(listener);
  await resumed.deleteLocal(small.remoteAttachment);
  assert.equal(existsSync(await resumed.localPath(remote)), false);
  // The reader has both deletions, so a live listener had its turn.
  for (const expected of [remote, small.remoteAttachment]) {
    const next = (await within(deleted.next(), "deletion")).value;
    if (next?.kind !== "attachmentDeleted") throw new Error("not a deletion");
    assert.equal(next.attachment.url, expected.url);
  }
  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(deletions, 1, "a stopped listener saw a deletion");
  await deleted.return();
  await reopened.end();
  await rm(root, { recursive: true, force: true });
  console.log("Node ATCH end: end waits for an upload; calls fail closed");
}
