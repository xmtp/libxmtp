// The Node binding's async shutdown boundary for attachments: Client.end()
// waits for an upload in flight, and held attachment handles then fail with
// the public ClientClosed. Rust owns the rules
// (attachment_flows.rs::attachment_calls_fail_closed_after_end).
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { clientOptions, createSigner, sleep } from "@test/helpers";
import { Client, XmtpError, type AttachmentSource } from "@xmtp/node-sdk";
import { expect, it, onTestFinished } from "vitest";

import { heldUploadBackend } from "./heldUpload";

/** Rejects when `promise` does not settle in time, so a deadlock fails. */
function within<T>(promise: Promise<T>, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 10_000);
    }),
  ]).finally(() => clearTimeout(timer));
}

const bytes = (text: string): AttachmentSource => ({
  kind: "bytes",
  bytes: new TextEncoder().encode(text),
  filename: "note.txt",
  mimeType: "text/plain",
});

it("end waits for a held upload, then attachment handles fail closed", async () => {
  const backend = await heldUploadBackend();
  onTestFinished(() => backend.close());
  const root = await realpath(
    await mkdtemp(join(tmpdir(), "xmtp-node-attachment-end-")),
  );
  const { identifier, signer } = createSigner();
  const options = clientOptions({
    backend: { url: backend.url },
    storage: { location: { directory: root } },
    attachments: { allowPrivateNetwork: true },
  });
  let client: Client | undefined;
  let reopened: Client | undefined;
  try {
    client = await Client.create(signer, options);
    const attachments = client.attachments;
    const events = await client.events({
      kinds: ["attachment.upload_started"],
    });
    const pending = await attachments.create(bytes("held upload"));
    const upload = pending.upload();
    await within(backend.entered, "held PUT");
    expect((await within(events.next(), "upload start")).value?.kind).toBe(
      "attachment.upload_started",
    );
    let ended = false;
    const ending = client.end().then(() => {
      ended = true;
    });
    // The ended event stream proves Rust has entered client shutdown.
    expect((await within(events.next(), "event end")).done).toBe(true);
    await sleep(250);
    expect(ended, "end returned while the PUT was held").toBe(false);
    expect(backend.counts()).toEqual({ puts: 1, grants: 1 });
    backend.release();
    await within(ending, "end after release");
    await within(upload, "upload across end");

    // Held values stay readable; every call fails with the public error.
    expect(attachments.offered).toBe(true);
    const remote = pending.remoteAttachment;
    const closedCalls: [string, () => Promise<unknown>][] = [
      ["create", () => attachments.create(bytes("late"))],
      ["localPath", () => attachments.localPath(remote)],
      ["listLocal", () => attachments.listLocal()],
      ["download", () => attachments.download(remote)],
      ["pending", () => attachments.pending(remote)],
      ["listPending", () => attachments.listPending()],
      ["deleteLocal", () => attachments.deleteLocal(remote)],
      ["handle localPath", () => pending.localPath()],
      ["handle status", () => pending.status()],
      ["handle upload", () => pending.upload()],
    ];
    for (const [name, call] of closedCalls) {
      const error: unknown = await within(call(), name).then(
        () => undefined,
        (failure: unknown) => failure,
      );
      expect(error, name).toBeInstanceOf(XmtpError.ClientClosed);
      expect(error, name).toMatchObject({
        details: { code: "ClientClosed", category: "lifecycle" },
      });
    }

    // The held handles do not keep the ended database open, and the upload
    // finished before end returned.
    reopened = await Client.build(identifier, options);
    expect(await (await reopened.attachments.pending(remote)).status()).toEqual(
      { kind: "complete" },
    );
    expect(backend.counts()).toEqual({ puts: 1, grants: 1 });
    await reopened.end();
  } finally {
    // After a failed step, end what is open before the files go. Ending an
    // ended client does nothing.
    backend.release();
    await Promise.allSettled([reopened?.end(), client?.end()]);
    await rm(root, { recursive: true, force: true });
  }
});
