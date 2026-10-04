// Attachments in the shipped worker build, against the backend's object
// store and the loopback store. Attachment paths name OPFS entries.
import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import {
  attachmentFilter,
  attachmentsDir,
  bytesSource,
  drain,
  existsOpfs,
  failure,
  fileOptions,
  readOpfs,
  readOpfsText,
  rejectsClosed,
  relay,
  removeOpfs,
  same,
  servedObject,
  sha256Hex,
  thrownError,
  thrownFailure,
  writeOpfs,
} from "./attachments-support";
import {
  build,
  connection,
  create,
  equal,
  expect,
  signer,
} from "./suite-support";

/** Server configuration and options, including 64-bit values. */
export async function checkAttachmentSettings(
  backendURL: string,
  baseUrl: string,
): Promise<void> {
  const { session, worker } = connection();
  try {
    const large = 2n ** 53n + 1n;
    const settings = {
      maxDownloadBytes: large,
      maxPendingAgeSeconds: large + 2n,
      allowPrivateNetwork: true,
    };
    const { client } = await create(
      session,
      signer(session),
      fileOptions(backendURL, `atch-${crypto.randomUUID()}`, settings),
    );
    same(
      client.serverConfiguration.attachments,
      { baseUrl, maxUploadBytes: 104_857_600n, retentionSeconds: 0n },
      "server attachment configuration",
    );
    same(client.options.attachments, settings, "attachment options");
    equal(
      client.options.attachments?.maxPendingAgeSeconds,
      large + 2n,
      "a 64-bit option lost precision",
    );
    equal(client.attachments.offered, true, "attachments not offered");
    await client.end();
  } finally {
    worker.terminate();
  }
}

/** Create, send, upload, reopen, resume, and download on another client. */
export async function checkAttachmentFlow(
  backendURL: string,
  store: string,
): Promise<void> {
  const { session, worker } = connection();
  try {
    const root = `atch-${crypto.randomUUID()}`;
    const owner = signer(session);
    const senderOptions = fileOptions(backendURL, `${root}/sender`);
    const { client: sender } = await create(session, owner, senderOptions);
    const { client: receiver } = await create(
      session,
      signer(session),
      fileOptions(backendURL, `${root}/receiver`),
    );
    const attachments = sender.attachments;
    const events = await sender.events(attachmentFilter());
    // The receiver's reader sees none of the sender's events.
    const receiverEvents = await receiver.events(attachmentFilter());

    const content = "attachment bytes";
    const fromBytes = await attachments.create(bytesSource(content));
    const remote = fromBytes.remoteAttachment;
    same(await fromBytes.status(), { kind: "waiting" }, "new status");
    equal(
      await readOpfsText(await fromBytes.localPath()),
      content,
      "staged plaintext",
    );
    equal(
      await attachments.localPath(remote),
      await fromBytes.localPath(),
      "derived path of a pending attachment",
    );
    // The SDK copies an OPFS path source at create, so removing it changes
    // nothing.
    const source = `${root}/photo.bin`;
    await writeOpfs(source, "path bytes");
    const fromPath = await attachments.create({
      kind: "path",
      path: source,
      filename: undefined,
      mimeType: "application/octet-stream",
    });
    await removeOpfs(source);
    const pathRemote = fromPath.remoteAttachment;
    equal(
      await readOpfsText(await fromPath.localPath()),
      "path bytes",
      "copied path source",
    );
    same(await drain(sender, events), [], "create sent events");

    // The record is complete before any upload, so the app sends it first.
    const dm = await sender.conversations.createDm(receiver.inboxId);
    const sent = await dm.sendRemoteAttachment(remote);
    // Concurrent uploads of one attachment share one transfer.
    await Promise.all([fromBytes.upload(), fromBytes.upload()]);
    same(await fromBytes.status(), { kind: "complete" }, "uploaded status");
    const uploaded = await drain(sender, events);
    same(
      uploaded.map((event) => event.kind),
      ["attachment.upload_started", "attachment.upload_completed"],
      "expected one shared upload",
    );
    same(uploaded[0]!.attachment, uploaded[1]!.attachment, "upload refs");
    equal(uploaded[0]!.attachment.url, remote.url, "upload event URL");
    equal(
      uploaded[0]!.attachment.contentDigest,
      remote.contentDigest,
      "upload event digest",
    );
    same(await drain(receiver, receiverEvents), [], "receiver saw events");
    await events.return();
    await sender.end();
    // Held values stay readable after end; calls fail closed.
    equal(attachments.offered, true, "offered after end");
    same(fromBytes.remoteAttachment, remote, "remote attachment after end");
    await rejectsClosed(fromPath.status(), "status after end");
    await rejectsClosed(attachments.listPending(), "listPending after end");

    // A reopened client lists the upload it did not finish and resumes it.
    const { client: reopened } = await build(
      session,
      await owner.identity(),
      senderOptions,
    );
    const resuming = reopened.attachments;
    const listed = await resuming.listPending();
    equal(listed.length, 1, "expected one pending upload");
    equal(
      listed[0]!.remoteAttachment.contentDigest,
      pathRemote.contentDigest,
      "listed pending upload",
    );
    same(
      await (await resuming.pending(remote)).status(),
      { kind: "complete" },
      "reopened complete status",
    );
    const resumed = await resuming.pending(pathRemote);
    same(await resumed.status(), { kind: "waiting" }, "reopened status");
    await resumed.upload();
    same(await resumed.status(), { kind: "complete" }, "resumed status");
    same(await resuming.listPending(), [], "pending after resume");
    same(
      await thrownFailure(
        resuming.pending({ ...pathRemote, contentDigest: "00".repeat(32) }),
      ),
      failure("stagedUnusable"),
      "pending with no staged data",
    );

    // The receiver derives the path of the record it was sent without a
    // request.
    await receiver.conversations.syncAll(undefined);
    const message = await receiver.conversations.getMessageById(sent);
    if (message?.content.kind !== "remoteAttachment")
      throw new Error(
        "the sent attachment did not arrive as a remote attachment",
      );
    const received = message.content.value;
    const receiving = receiver.attachments;
    const directory = attachmentsDir(await receiver.storage.path());
    const probe = relay(store);
    await probe.refuse();
    const derived = await receiving.localPath({
      ...received,
      url: `${probe.url}/object`,
    });
    equal(await probe.requests(), 0, "path derivation sent a request");
    expect(derived.startsWith(`${directory}/`), `derived path ${derived}`);
    const expectedPath = await receiving.localPath(received);
    expect(!(await existsOpfs(expectedPath)), "downloaded before download");

    // Download events arrive in order; a filtered reader sees only its kind.
    const deletedOnly = await receiver.events(
      attachmentFilter(["attachment.deleted"]),
    );
    const downloaded = await receiving.download(received);
    same(
      downloaded,
      { path: expectedPath, mimeType: "text/plain", filename: "note.txt" },
      "downloaded attachment",
    );
    equal(await readOpfsText(downloaded.path), content, "downloaded bytes");
    const pathDownload = await receiving.download(pathRemote);
    equal(
      await readOpfsText(pathDownload.path),
      "path bytes",
      "downloaded path source",
    );
    const local = await receiving.listLocal();
    expect(
      local.every((file) => file.createdAt instanceof sdk.Timestamp),
      "local attachment time is not a Timestamp",
    );
    same(
      local.map((file) => file.path).sort(),
      [downloaded.path, pathDownload.path]
        .map((path) => path.slice(directory.length + 1))
        .sort(),
      "local attachment paths",
    );
    await receiving.deleteLocal(received);
    equal((await receiving.listLocal()).length, 1, "local after delete");
    expect(!(await existsOpfs(downloaded.path)), "deleted file remains");
    const downloads = await drain(receiver, receiverEvents);
    same(
      downloads.map((event) => [event.kind, event.attachment.contentDigest]),
      [
        ["attachment.download_started", received.contentDigest],
        ["attachment.download_completed", received.contentDigest],
        ["attachment.download_started", pathRemote.contentDigest],
        ["attachment.download_completed", pathRemote.contentDigest],
        ["attachment.deleted", received.contentDigest],
      ],
      "download events",
    );
    equal(downloads[0]!.attachment.url, received.url, "download event URL");
    expect(
      downloads[0]!.attachment.attachmentKey !==
        downloads[2]!.attachment.attachmentKey,
      "two downloads share a key",
    );
    same(downloads[4]!.attachment, downloads[0]!.attachment, "deleted ref");
    same(await drain(receiver, deletedOnly), [downloads[4]], "filtered reader");
    await deletedOnly.return();
    await receiverEvents.return();
    await reopened.end();
    await receiver.end();
  } finally {
    worker.terminate();
  }
}

/** Failed uploads and downloads carry one record in errors and status. */
export async function checkAttachmentFailures(
  backendURL: string,
  store: string,
): Promise<void> {
  const { session, worker } = connection();
  try {
    const root = `atch-${crypto.randomUUID()}`;
    const { client } = await create(
      session,
      signer(session),
      fileOptions(backendURL, root),
    );
    const attachments = client.attachments;
    const events = await client.events(attachmentFilter());
    const staged = `${attachmentsDir(await client.storage.path())}/.staged`;

    // Missing staged data fails the upload before any request.
    const pending = await attachments.create(bytesSource("staged"));
    const remote = pending.remoteAttachment;
    const ciphertext = await readOpfs(`${staged}/${remote.contentDigest}`);
    await removeOpfs(`${staged}/${remote.contentDigest}`);
    const other = await attachments.create(bytesSource("other"));
    const otherRemote = other.remoteAttachment;
    const otherCiphertext = await readOpfs(
      `${staged}/${otherRemote.contentDigest}`,
    );
    const thrown = await thrownFailure(pending.upload());
    same(thrown, failure("stagedUnusable"), "missing staged data");
    same(
      await pending.status(),
      { kind: "failed", value: thrown },
      "failed status",
    );
    const failed = await drain(client, events);
    same(
      failed.map((event) => event.kind),
      ["attachment.upload_started", "attachment.upload_failed"],
      "upload failure events",
    );
    same(
      failed[1]!.attachment,
      { ...failed[0]!.attachment, cause: "stagedUnusable" },
      "upload failure ref",
    );
    // A source the SDK cannot read fails create.
    equal(
      (
        await thrownFailure(
          attachments.create({
            kind: "path",
            path: `${root}/missing.bin`,
            filename: undefined,
            mimeType: "application/octet-stream",
          }),
        )
      ).cause,
      "sourceUnreadable",
      "missing OPFS source",
    );
    await events.return();

    // The creating client holds the plaintext, so another client downloads.
    const { client: downloader } = await create(
      session,
      signer(session),
      fileOptions(backendURL, `${root}/downloader`),
    );
    const downloads = downloader.attachments;
    const downloadEvents = await downloader.events(attachmentFilter());
    const unavailable = relay(store);
    await unavailable.refuse();
    const error = await thrownError(
      downloads.download({ ...remote, url: `${unavailable.url}/object` }),
    );
    same(
      error.attachmentFailure,
      failure("httpStatus", { httpStatus: 503 }),
      "unavailable download",
    );
    equal(error.details.category, "network", "503 category");
    equal(error.details.retryable, true, "503 retry");
    equal(await unavailable.requests(), 1, "the SDK retried");
    // The browser does not follow a redirect.
    const redirected = await thrownError(
      downloads.download({ ...remote, url: `${store}/redirect` }),
    );
    same(
      redirected.attachmentFailure,
      failure("tooManyRedirects"),
      "redirected download",
    );
    equal(redirected.details.category, "network", "redirect category");
    equal(redirected.details.retryable, false, "redirect retry");

    // Another attachment's object decrypts and decodes, so only its digest
    // differs from the record.
    equal(
      (
        await thrownFailure(
          downloads.download({
            ...otherRemote,
            url: await servedObject(store, otherCiphertext),
            contentDigest: remote.contentDigest,
          }),
        )
      ).cause,
      "digestMismatch",
      "substituted object",
    );
    // A changed tag byte with a matching digest fails only the decryption.
    const tamperedBytes = Uint8Array.from(ciphertext);
    tamperedBytes[tamperedBytes.length - 1]! ^= 1;
    const tampered = await servedObject(store, tamperedBytes);
    equal(
      (
        await thrownFailure(
          downloads.download({
            ...remote,
            url: tampered,
            contentDigest: await sha256Hex(tamperedBytes),
          }),
        )
      ).cause,
      "decryptionFailed",
      "tampered object",
    );
    equal(
      (
        await thrownFailure(
          downloads.download({
            ...remote,
            url: tampered,
            secret: new Uint8Array([7, 7, 7]),
          }),
        )
      ).cause,
      "malformed",
      "malformed record",
    );
    const downloadFailures = await drain(downloader, downloadEvents);
    same(
      downloadFailures.flatMap((event) =>
        event.kind === "attachment.download_failed"
          ? [event.attachment.cause]
          : [],
      ),
      ["httpStatus", "tooManyRedirects", "digestMismatch", "decryptionFailed"],
      "download failure events",
    );
    await downloadEvents.return();
    await downloader.end();
    await client.end();
  } finally {
    worker.terminate();
  }
}
