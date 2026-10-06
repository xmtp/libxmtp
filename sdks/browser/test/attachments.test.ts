// The browser download path: WASM fetch from the backend's object store,
// digest check, decryption, and the plaintext file in OPFS. The Rust download
// tests do not build for wasm32, so these are the only real browser downloads.
import type { AttachmentSource, ClientOptions } from "@xmtp/browser-sdk";
import { expect, test } from "vitest";
import { commands } from "vitest/browser";

import { create, signer } from "./helpers";

// The local object store is on a loopback address.
const fileClient = (directory: string): Partial<ClientOptions> => ({
  storage: { location: { directory } },
  attachments: { allowPrivateNetwork: true },
});

const source = (text: string): AttachmentSource => ({
  kind: "bytes",
  bytes: new TextEncoder().encode(text),
  filename: "note.txt",
  mimeType: "text/plain",
});

/** An attachment path names an OPFS entry. */
async function opfsFile(path: string): Promise<File | undefined> {
  const names = path.split("/").filter((name) => name !== "");
  const name = names.pop()!;
  try {
    let directory = await navigator.storage.getDirectory();
    for (const part of names)
      directory = await directory.getDirectoryHandle(part);
    return await (await directory.getFileHandle(name)).getFile();
  } catch (error) {
    if (error instanceof DOMException && error.name === "NotFoundError")
      return undefined;
    throw error;
  }
}

// verifies: ATCH-048
test("a peer downloads an uploaded attachment into OPFS and deletes it", async () => {
  const root = `attachments-${crypto.randomUUID()}`;
  const sender = await create(signer(), fileClient(`${root}/sender`));
  const receiver = await create(signer(), fileClient(`${root}/receiver`));
  const content = "attachment bytes";
  const pending = await sender.attachments.create(source(content));
  const other = await sender.attachments.create(source("other bytes"));
  const dm = await sender.conversations.createDm(receiver.inboxId);
  const sent = await dm.sendRemoteAttachment(pending.remoteAttachment);
  await pending.upload();
  await other.upload();

  await receiver.conversations.syncAll(undefined);
  const message = await receiver.conversations.getMessageById(sent);
  if (message?.content.kind !== "remoteAttachment")
    throw new Error("the attachment record did not arrive");
  const received = message.content.value;
  const attachments = receiver.attachments;
  const path = await attachments.localPath(received);
  expect(await opfsFile(path)).toBeUndefined();

  const downloaded = await attachments.download(received);
  expect(downloaded).toEqual({
    path,
    mimeType: "text/plain",
    filename: "note.txt",
  });
  expect(await (await opfsFile(path))?.text()).toBe(content);
  // Local paths are relative to the attachments directory.
  const local = await attachments.listLocal();
  expect(local).toHaveLength(1);
  expect(path.endsWith(`/${local[0]!.path}`)).toBe(true);

  // Another object under this record's digest fails the digest check, and
  // no file is written for it.
  const substituted = {
    ...other.remoteAttachment,
    contentDigest: received.contentDigest,
  };
  await expect(attachments.download(substituted)).rejects.toMatchObject({
    attachmentFailure: { cause: "digestMismatch" },
  });
  expect(await opfsFile(await attachments.localPath(substituted))).toBe(
    undefined,
  );

  await attachments.deleteLocal(received);
  expect(await opfsFile(path)).toBeUndefined();
  expect(await attachments.listLocal()).toEqual([]);
});

// verifies: ATCH-055
// verifies: ATCH-057
// verifies: ATCH-079
test("a browser download fails typed on a redirect and on a failure status", async () => {
  const root = `attachments-${crypto.randomUUID()}`;
  const sender = await create(signer(), fileClient(`${root}/sender`));
  const receiver = await create(signer(), fileClient(`${root}/receiver`));
  const record = (await sender.attachments.create(source("bytes")))
    .remoteAttachment;
  const host = await commands.startDownloadHost();
  try {
    const download = (path: string) =>
      receiver.attachments.download({ ...record, url: `${host.url}${path}` });
    // Manual redirect handling: the browser gives the worker an opaque
    // redirect and does not request the target.
    await expect(download("/redirect")).rejects.toMatchObject({
      attachmentFailure: { cause: "tooManyRedirects", httpStatus: undefined },
    });
    expect(await commands.downloadHostRequests(host.id)).toEqual(["/redirect"]);
    await expect(download("/unavailable")).rejects.toMatchObject({
      attachmentFailure: { cause: "httpStatus", httpStatus: 503 },
    });
    await expect(download("/gone")).rejects.toMatchObject({
      attachmentFailure: { cause: "notFound" },
    });
  } finally {
    await commands.closeDownloadHost(host.id);
  }
});

// verifies: ATCH-051
test("a browser download with a changed tag and a matching digest fails decryption and writes no file", async () => {
  const root = `attachments-${crypto.randomUUID()}`;
  const sender = await create(signer(), fileClient(`${root}/sender`));
  const receiver = await create(signer(), fileClient(`${root}/receiver`));
  const pending = await sender.attachments.create(source("attachment bytes"));
  await pending.upload();
  const host = await commands.startDownloadHost();
  try {
    const { path, contentDigest } = await commands.serveTamperedObject(
      host.id,
      pending.remoteAttachment.url,
    );
    // The digest matches the served bytes, so only the tag check rejects them.
    const tampered = {
      ...pending.remoteAttachment,
      url: `${host.url}${path}`,
      contentDigest,
    };
    await expect(receiver.attachments.download(tampered)).rejects.toMatchObject(
      { attachmentFailure: { cause: "decryptionFailed" } },
    );
    expect(await commands.downloadHostRequests(host.id)).toEqual([path]);
    expect(
      await opfsFile(await receiver.attachments.localPath(tampered)),
    ).toBeUndefined();
    expect(await receiver.attachments.listLocal()).toEqual([]);
  } finally {
    await commands.closeDownloadHost(host.id);
  }
});
