import * as sdk from "../../../../target/sdk-generated/typescript-wasm/index";
import { currentProjection } from "../../../../target/sdk-generated/typescript-wasm/public-values.gen";
// Lifetime proofs use the generated package's worker and OPFS manager.
import { heldTransfer } from "../ts/transfer-control.mts";
import { controlPackageWorker, signal } from "./attachment-worker-control";
import {
  attachmentFilter,
  bytesSource,
  existsOpfs,
  fileOptions,
  rejectsClosed,
  same,
  within,
  writeOpfs,
  readOpfsText,
  thrownFailure,
  failure,
} from "./attachments-support";
import { equal, expect, signer } from "./suite-support";

// verifies: EVENT-053, EVENT-054
export async function checkEventEnd(backendURL: string): Promise<void> {
  const control = controlPackageWorker();
  let client: sdk.Client | undefined;
  try {
    client = await sdk.Client.create(
      signer(),
      fileOptions(backendURL, `event-end-${crypto.randomUUID()}`),
    );
    const events = await client.events({
      kinds: ["conversation.joined"],
      referencesOwnMessages: false,
    });
    control.worker.holdEvent();
    const read = events.next();
    void read.catch(() => {});
    await client.conversations.createGroup([]);
    await within(control.worker.arrived.promise, "held event reply");
    await within(client.end(), "end with held event reply");
    // Close settles the app read even before the delayed reply arrives.
    same(
      await within(read, "event read at end"),
      { done: true },
      "normal event end",
    );
    control.worker.release();
    same(await events.next(), { done: true }, "later event read");
  } finally {
    try {
      await client?.end();
      if (client)
        await within(control.terminated, "package worker termination");
    } finally {
      control.restore();
    }
  }
}

/** A held PUT keeps storage alive through end and transport cancellation. */
export async function checkAttachmentEnd(
  store: string,
  cancel = false,
): Promise<void> {
  const held = await heldTransfer(store);
  const initialControl = controlPackageWorker();
  const owner = signer();
  const options = fileOptions(held.backend, `atch-end-${crypto.randomUUID()}`);
  const client = await sdk.Client.create(owner, options).catch(
    (error: unknown) => {
      initialControl.restore();
      throw error;
    },
  );
  try {
    const attachments = client.attachments;
    const small = await attachments.create(bytesSource("small"));
    const pending = await attachments.create(bytesSource("held upload"));
    const remote = pending.remoteAttachment;
    const events = await client.events(
      attachmentFilter(["attachment.upload_started"]),
    );
    const abort = new AbortController();
    // Public upload has no AbortSignal option. This case cancels the actual
    // generated binding transport, which drops the waiting Rust future.
    const upload = cancel
      ? currentProjection()
          .lowerPendingAttachment(pending)
          .upload({ signal: abort.signal })
      : pending.upload();
    const outcome = upload.then(
      () => "complete",
      () => "cancelled",
    );
    await within(held.command("entered"), "held PUT");
    equal(
      (await within(events.next(), "upload start")).value?.kind,
      "attachment.upload_started",
      "upload start",
    );
    if (cancel) {
      abort.abort();
      equal(
        await within(outcome, "cancel upload caller"),
        "cancelled",
        "upload caller cancellation",
      );
    }
    const readPosted = initialControl.worker.watchEventRead();
    const waiting = events.next();
    await within(readPosted, "event read posted before end");
    let ended = false;
    const ending = client.end().then(() => {
      ended = true;
    });
    void ending.catch(() => {});
    // Rust closes subscriptions before it waits for the running transfer.
    equal(
      (await within(waiting, "end starts")).done,
      true,
      "event read at end",
    );
    expect(!ended, "end released storage while PUT was held");
    same(
      await (await held.command("counts")).json(),
      { puts: 1, grants: 1, gets: 0 },
      "held transfer requests",
    );
    await held.command("release");
    await within(ending, "end after release");
    if (!cancel) equal(await outcome, "complete", "upload across end");
    equal(attachments.offered, true, "offered after end");
    for (const [label, call] of [
      ["create", () => attachments.create(bytesSource("late"))],
      ["localPath", () => attachments.localPath(remote)],
      ["listLocal", () => attachments.listLocal()],
      ["download", () => attachments.download(remote)],
      ["pending", () => attachments.pending(remote)],
      ["listPending", () => attachments.listPending()],
      ["deleteLocal", () => attachments.deleteLocal(remote)],
      ["pending localPath", () => pending.localPath()],
      ["status", () => pending.status()],
      ["upload", () => pending.upload()],
    ] as const)
      await rejectsClosed(call(), `${label} after end`);

    await within(initialControl.terminated, "ended package worker termination");
    initialControl.restore();
    const control = controlPackageWorker();
    let reopened: sdk.Client | undefined;
    try {
      reopened = await sdk.Client.build(await owner.identity(), options);
      const resumed = reopened.attachments;
      same(
        await (await resumed.pending(remote)).status(),
        { kind: "complete" },
        "persisted completion after end",
      );
      same(
        await (await held.command("counts")).json(),
        { puts: 1, grants: 1, gets: 0 },
        "reopen did not upload again",
      );
      let deletions = 0;
      const first = signal();
      const listener = await reopened.startListener(
        { kinds: ["attachment.deleted"], referencesOwnMessages: false },
        () => {
          deletions++;
          first.resolve();
        },
      );
      const deleted = await reopened.events(
        attachmentFilter(["attachment.deleted"]),
      );
      await resumed.deleteLocal(remote);
      await within(first.promise, "first deletion callback");
      equal(deletions, 1, "listener deletions");
      control.worker.holdListener();
      await resumed.deleteLocal(small.remoteAttachment);
      await within(control.worker.arrived.promise, "held deletion callback");
      await reopened.stopListener(listener);
      control.worker.release();
      await within(
        control.worker.callbackFinished.promise,
        "stopped callback dispatch",
      );
      equal(deletions, 1, "a stopped listener saw a deletion");
      expect(
        !(await existsOpfs(await resumed.localPath(remote))),
        "deleted file remains",
      );
      for (const expected of [remote, small.remoteAttachment]) {
        const event = (await within(deleted.next(), "deletion")).value;
        if (event?.kind !== "attachment.deleted")
          throw new Error("not a deletion");
        equal(event.attachment_deleted.url, expected.url, "deletion order");
      }
      await deleted.return();
    } finally {
      await reopened?.end();
      if (reopened)
        await within(control.terminated, "reopened worker termination");
      control.restore();
    }
  } finally {
    await held.command("release");
    await client.end();
    initialControl.restore();
  }
}

async function workerFailure(operation: Promise<unknown>, label: string) {
  try {
    await within(operation, label);
  } catch (error) {
    expect(
      error instanceof sdk.XmtpError.Unknown,
      `${label}: expected Unknown`,
    );
    same(
      error.details,
      {
        code: "Unknown",
        category: "lifecycle",
        retryable: false,
        message: "workerTerminated",
      },
      label,
    );
    return;
  }
  throw new Error(`${label} did not fail`);
}

/** Worker death releases the actual package reservation, without replay. */
export async function checkAttachmentWorkerDeath(
  store: string,
  terminate = true,
): Promise<void> {
  const held = await heldTransfer(store);
  const owner = signer();
  const options = fileOptions(
    held.backend,
    `atch-death-${crypto.randomUUID()}`,
  );
  const control = controlPackageWorker();
  let client: sdk.Client | undefined;
  try {
    client = await sdk.Client.create(owner, options);
    const attachments = client.attachments;
    const pending = await attachments.create(bytesSource("held"));
    const remote = pending.remoteAttachment;
    const events = await client.events(attachmentFilter());
    const inFlight = attachments.download({
      ...remote,
      url: `${held.url}/download`,
      contentDigest: "11".repeat(32),
    });
    void inFlight.catch(() => {});
    await within(held.command("download-entered"), "held download");
    equal(
      (await within(events.next(), "download start")).value?.kind,
      "attachment.download_started",
      "download start",
    );
    const waiting = events.next();
    void waiting.catch(() => {});
    control.worker.fail(terminate);
    await workerFailure(inFlight, "download in flight");
    await workerFailure(waiting, "waiting event reader");
    for (const [label, call] of [
      ["create", () => attachments.create(bytesSource("late"))],
      ["localPath", () => attachments.localPath(remote)],
      ["listPending", () => attachments.listPending()],
      ["status", () => pending.status()],
      ["upload", () => pending.upload()],
    ] as const)
      await rejectsClosed(call(), `${label} after worker death`);
    equal(attachments.offered, true, "offered after worker death");
    equal(client.attachments.offered, true, "attachments after worker death");
    same(pending.remoteAttachment, remote, "record after worker death");
    equal(control.worker.terminationCalls, 1, "package worker termination");
    await within(control.terminated, "package terminates worker");
    control.restore();
    // No manual session or storage release occurs before this package call.
    const replacementControl = controlPackageWorker();
    const reopened = await sdk.Client.build(await owner.identity(), options);
    try {
      same(
        await (await reopened.attachments.pending(remote)).status(),
        { kind: "waiting" },
        "pending record survives worker death",
      );
      same(
        await (await held.command("counts")).json(),
        { puts: 0, grants: 0, gets: 1 },
        "worker replacement did not replay a transfer",
      );
    } finally {
      await reopened.end();
      await within(
        replacementControl.terminated,
        "replacement worker termination",
      );
      replacementControl.restore();
    }
  } finally {
    control.terminateFixture();
    control.restore();
    await held.command("release");
    await client?.end().catch(() => {});
  }
}

/** Malformed sources fail before any attachment call enters the OPFS worker. */
export async function checkAttachmentSourceShape(
  backendURL: string,
): Promise<void> {
  const control = controlPackageWorker();
  let client: sdk.Client | undefined;
  try {
    await writeOpfs("undefined", "do not read this by accident");
    client = await sdk.Client.create(
      signer(),
      fileOptions(backendURL, `atch-shape-${crypto.randomUUID()}`),
    );
    const attachments = client.attachments;
    // Prove the entry exists and the correctly shaped path can read it.
    const valid = await attachments.create({
      kind: "path",
      path: "undefined",
      mimeType: "text/plain",
      filename: undefined,
    });
    equal(
      await readOpfsText(await valid.localPath()),
      "do not read this by accident",
      "OPFS source control",
    );
    const before = control.worker.attachmentCreates;
    const local = await attachments.listLocal();
    const pending = (await attachments.listPending()).map(
      (item) => item.remoteAttachment,
    );
    for (const source of [
      { kind: "path", mimeType: "text/plain" },
      { kind: "path", path: undefined, mimeType: "text/plain" },
      { kind: "bytes", mimeType: "text/plain" },
      { kind: "bytes", bytes: undefined, mimeType: "text/plain" },
      {
        kind: "path",
        path: "undefined",
        bytes: new Uint8Array([1]),
        mimeType: "text/plain",
      },
      {
        kind: "bytes",
        bytes: new Uint8Array([1]),
        path: "undefined",
        mimeType: "text/plain",
      },
    ]) {
      same(
        await thrownFailure(
          attachments.create(source as unknown as sdk.AttachmentSource),
        ),
        failure("malformed"),
        "source shape failure",
      );
    }
    equal(
      control.worker.attachmentCreates,
      before,
      "malformed source reached the OPFS worker",
    );
    same(
      await attachments.listLocal(),
      local,
      "malformed source wrote a local file",
    );
    same(
      (await attachments.listPending()).map((item) => item.remoteAttachment),
      pending,
      "malformed source wrote a pending record",
    );
    equal(
      await readOpfsText("undefined"),
      "do not read this by accident",
      "source changed",
    );
  } finally {
    try {
      await client?.end();
      if (client)
        await within(control.terminated, "package worker termination");
    } finally {
      control.restore();
    }
  }
}
