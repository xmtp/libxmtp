import type { Client, RemoteAttachment } from "@xmtp/browser-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  downloadRemoteAttachment,
  uploadEncryptedAttachment,
} from "./attachment";

describe("remote attachments", () => {
  it("reads the SDK download path and preserves its MIME type", async () => {
    const directory = await navigator.storage.getDirectory();
    const name = `web-chat-${crypto.randomUUID()}.bin`;
    const handle = await directory.getFileHandle(name, { create: true });
    const writer = await handle.createWritable();
    await writer.write("hello from xmtp.chat");
    await writer.close();
    expect((await handle.getFile()).type).not.toBe("text/plain");
    const remote = {
      url: "https://example.com/attachment",
    } as RemoteAttachment;
    const download = vi.fn(async () => ({
      path: name,
      mimeType: "text/plain",
    }));
    const deleteLocal = vi.fn(async () => directory.removeEntry(name));
    const client = {
      attachments: { download, deleteLocal },
    } as unknown as Client;
    try {
      const result = await downloadRemoteAttachment(client, remote);
      expect(download).toHaveBeenCalledExactlyOnceWith(remote);
      expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
      await expect(directory.getFileHandle(name)).rejects.toMatchObject({
        name: "NotFoundError",
      });
      expect(await result.text()).toBe("hello from xmtp.chat");
      expect(result.type).toBe("text/plain");
    } finally {
      await directory.removeEntry(name).catch(() => {});
    }
  });
});

it("creates and uploads through the SDK before returning the remote record", async () => {
  const file = new File(["payload"], "photo.png", { type: "image/png" });
  const remote = { url: "https://example.com/attachment" } as RemoteAttachment;
  const held = Promise.withResolvers<void>();
  const upload = vi.fn(() => held.promise);
  const create = vi.fn(async () => ({ upload, remoteAttachment: remote }));
  const deleteLocal = vi.fn(async () => {});
  const client = { attachments: { create, deleteLocal } } as unknown as Client;
  let done = false;
  const result = uploadEncryptedAttachment(client, file).then((value) => {
    done = true;
    return value;
  });
  await vi.waitFor(() => expect(upload).toHaveBeenCalledOnce());
  expect(done).toBe(false);
  expect(create).toHaveBeenCalledExactlyOnceWith({
    kind: "bytes",
    bytes: new TextEncoder().encode("payload"),
    mimeType: "image/png",
    filename: "photo.png",
  });
  held.resolve();
  expect(await result).toBe(remote);
  expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
});

it("deletes the local attachment when upload fails", async () => {
  const remote = { url: "https://example.com/failed" } as RemoteAttachment;
  const failure = new Error("upload failed");
  const deleteLocal = vi.fn(async () => {});
  const client = {
    attachments: {
      create: vi.fn(async () => ({
        upload: async () => Promise.reject(failure),
        remoteAttachment: remote,
      })),
      deleteLocal,
    },
  } as unknown as Client;
  const file = new File(["payload"], "photo.png", { type: "image/png" });
  await expect(uploadEncryptedAttachment(client, file)).rejects.toBe(failure);
  expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
});
