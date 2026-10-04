import type {
  Client,
  PendingAttachment,
  RemoteAttachment,
} from "@xmtp/browser-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  cleanAttachmentDirectory,
  downloadRemoteAttachment,
  pendingAttachmentCleanupPaths,
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

it("does not use an invalid cleanup journal path for OPFS removal", async () => {
  const key = "XMTP_PENDING_ATTACHMENT_CLEANUP";
  const malformed = "xmtp-sdk/test/../other/xmtp.db3";
  localStorage.setItem(key, JSON.stringify([malformed]));
  const getDirectory = vi.spyOn(navigator.storage, "getDirectory");
  try {
    expect(pendingAttachmentCleanupPaths()).toEqual([]);
    await expect(cleanAttachmentDirectory(malformed)).rejects.toThrow(
      "Invalid local database path",
    );
    expect(getDirectory).not.toHaveBeenCalled();
  } finally {
    localStorage.removeItem(key);
    getDirectory.mockRestore();
  }
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

it("keeps the local attachment when upload fails", async () => {
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
  expect(deleteLocal).not.toHaveBeenCalled();
});

it("retries a failed upload with the same staged attachment", async () => {
  const remote = { url: "https://example.com/retry" } as RemoteAttachment;
  const upload = vi
    .fn()
    .mockRejectedValueOnce(new Error("network failed"))
    .mockResolvedValueOnce(undefined);
  const pending = {
    upload,
    remoteAttachment: remote,
  } as unknown as PendingAttachment;
  const create = vi.fn(async () => pending);
  const deleteLocal = vi.fn(async () => {});
  const client = { attachments: { create, deleteLocal } } as unknown as Client;
  const file = new File(["payload"], "photo.png", { type: "image/png" });
  const pendingRef: { current: PendingAttachment | null } = { current: null };

  await expect(
    uploadEncryptedAttachment(client, file, pendingRef),
  ).rejects.toThrow("network failed");
  expect(pendingRef.current).toBe(pending);
  await expect(
    uploadEncryptedAttachment(client, file, pendingRef),
  ).resolves.toBe(remote);
  expect(create).toHaveBeenCalledOnce();
  expect(upload).toHaveBeenCalledTimes(2);
  expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
});

it("returns a completed upload when local cleanup fails", async () => {
  const remote = { url: "https://example.com/uploaded" } as RemoteAttachment;
  const upload = vi.fn(async () => {});
  const deleteLocal = vi.fn().mockRejectedValue(new Error("OPFS busy"));
  const client = {
    attachments: {
      create: vi.fn(async () => ({ upload, remoteAttachment: remote })),
      deleteLocal,
    },
  } as unknown as Client;
  const file = new File(["payload"], "photo.png", { type: "image/png" });
  await expect(uploadEncryptedAttachment(client, file)).resolves.toBe(remote);
  expect(upload).toHaveBeenCalledOnce();
  expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
});

it("returns downloaded bytes when local cleanup fails", async () => {
  const directory = await navigator.storage.getDirectory();
  const name = `web-chat-${crypto.randomUUID()}.bin`;
  const handle = await directory.getFileHandle(name, { create: true });
  const writer = await handle.createWritable();
  await writer.write("downloaded bytes");
  await writer.close();
  const remote = { url: "https://example.com/downloaded" } as RemoteAttachment;
  const deleteLocal = vi.fn().mockRejectedValue(new Error("OPFS busy"));
  const client = {
    attachments: {
      download: vi.fn(async () => ({ path: name, mimeType: "text/plain" })),
      deleteLocal,
    },
  } as unknown as Client;
  try {
    const blob = await downloadRemoteAttachment(client, remote);
    expect(await blob.text()).toBe("downloaded bytes");
    expect(blob.type).toBe("text/plain");
    expect(deleteLocal).toHaveBeenCalledExactlyOnceWith(remote);
  } finally {
    await directory.removeEntry(name).catch(() => {});
  }
});
