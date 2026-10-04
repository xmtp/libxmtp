import {
  Storage,
  type Client,
  type PendingAttachment,
  type RemoteAttachment,
} from "@xmtp/browser-sdk";
import { describe, expect, it, vi } from "vitest";

import {
  cleanAttachmentDirectory,
  cleanSessionAttachments,
  cleanStoredSessionAttachments,
  deploymentComponent,
  deploymentHash,
  downloadRemoteAttachment,
  pendingAttachmentCleanupPaths,
  uploadEncryptedAttachment,
} from "./attachment";

it("uses the SDK deployment file name for the complete identifier", async () => {
  for (const [identifier, name] of [
    ["selected-deployment", "selected-deployment"],
    ["https://example.com/a", "a"],
    ["CON.txt", "_con.txt"],
    ["X".repeat(256), "x".repeat(190)],
  ]) {
    expect(await deploymentComponent(identifier)).toBe(
      `${name}-${await deploymentHash(identifier)}`,
    );
  }
});

it("cleans recorded plaintext and keeps an unknown deployment directory", async () => {
  const label = `scope-${crypto.randomUUID()}`;
  const identifier = "selected-deployment";
  const selected = await deploymentComponent(identifier);
  const forged = `forged-${await deploymentHash(identifier)}`;
  const inbox = "a".repeat(64);
  const plaintext = "b".repeat(64);
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle(label, { create: true });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com/b": identifier },
    }),
  );
  await writer.close();
  const attachments = async (name: string) => {
    const deployment = await backend.getDirectoryHandle(name, { create: true });
    const inboxDir = await deployment.getDirectoryHandle(inbox, {
      create: true,
    });
    const directory = await inboxDir.getDirectoryHandle("attachments", {
      create: true,
    });
    await directory.getDirectoryHandle(plaintext, { create: true });
    await directory.getDirectoryHandle(".staged", { create: true });
    return directory;
  };
  const selectedAttachments = await attachments(selected);
  const forgedAttachments = await attachments(forged);
  try {
    await cleanStoredSessionAttachments();
    await expect(
      selectedAttachments.getDirectoryHandle(plaintext),
    ).rejects.toMatchObject({ name: "NotFoundError" });
    await expect(
      selectedAttachments.getDirectoryHandle(".staged"),
    ).resolves.toBeDefined();
    await expect(
      forgedAttachments.getDirectoryHandle(plaintext),
    ).resolves.toBeDefined();
  } finally {
    await sdk.removeEntry(label, { recursive: true });
  }
});

it("keeps legacy plaintext without a listed database", async () => {
  const dbPath = `xmtp-${crypto.randomUUID()}-${"a".repeat(64)}.db3`;
  const plaintext = "b".repeat(64);
  const root = await navigator.storage.getDirectory();
  const attachments = await root.getDirectoryHandle(`${dbPath}.attachments`, {
    create: true,
  });
  await attachments.getDirectoryHandle(plaintext, { create: true });
  try {
    await cleanStoredSessionAttachments();
    await expect(
      attachments.getDirectoryHandle(plaintext),
    ).resolves.toBeDefined();
  } finally {
    await root.removeEntry(`${dbPath}.attachments`, { recursive: true });
  }
});

it("cleans legacy plaintext for a listed database", async () => {
  const dbPath = `xmtp-${crypto.randomUUID()}-${"a".repeat(64)}.db3`;
  const plaintext = "b".repeat(64);
  const root = await navigator.storage.getDirectory();
  await root.getFileHandle(dbPath, { create: true });
  const attachments = await root.getDirectoryHandle(`${dbPath}.attachments`, {
    create: true,
  });
  await attachments.getDirectoryHandle(plaintext, { create: true });
  const listFiles = vi.fn(async () => [dbPath]);
  const end = vi.fn(async () => {});
  const admin = vi
    .spyOn(Storage, "admin")
    .mockResolvedValue({ listFiles, end } as unknown as Awaited<
      ReturnType<typeof Storage.admin>
    >);
  try {
    await cleanStoredSessionAttachments();
    expect(listFiles).toHaveBeenCalledOnce();
    expect(end).toHaveBeenCalledOnce();
    await expect(
      attachments.getDirectoryHandle(plaintext),
    ).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    admin.mockRestore();
    await root.removeEntry(`${dbPath}.attachments`, { recursive: true });
    await root.removeEntry(dbPath);
  }
});

it("admits only recorded current paths from the cleanup journal", async () => {
  const label = `journal-${crypto.randomUUID()}`;
  const identifier = "selected-deployment";
  const selected = await deploymentComponent(identifier);
  const forged = `forged-${await deploymentHash(identifier)}`;
  const inbox = "a".repeat(64);
  const selectedPath = `xmtp-sdk/${label}/${selected}/${inbox}/xmtp.db3`;
  const forgedPath = `xmtp-sdk/${label}/${forged}/${inbox}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle(label, { create: true });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com/b": identifier },
    }),
  );
  await writer.close();
  localStorage.setItem(
    "XMTP_PENDING_ATTACHMENT_CLEANUP",
    JSON.stringify([selectedPath, forgedPath]),
  );
  try {
    expect(await pendingAttachmentCleanupPaths()).toEqual([selectedPath]);
  } finally {
    localStorage.removeItem("XMTP_PENDING_ATTACHMENT_CLEANUP");
    await sdk.removeEntry(label, { recursive: true });
  }
});

it("replays legacy cleanup only for a listed database", async () => {
  const listedPath = `xmtp-${crypto.randomUUID()}-${"a".repeat(64)}.db3`;
  const unlistedPath = `xmtp-${crypto.randomUUID()}-${"b".repeat(64)}.db3`;
  const plaintext = "c".repeat(64);
  const root = await navigator.storage.getDirectory();
  const listed = await root.getDirectoryHandle(`${listedPath}.attachments`, {
    create: true,
  });
  const unlisted = await root.getDirectoryHandle(
    `${unlistedPath}.attachments`,
    {
      create: true,
    },
  );
  await listed.getDirectoryHandle(plaintext, { create: true });
  await unlisted.getDirectoryHandle(plaintext, { create: true });
  const listFiles = vi.fn(async () => [listedPath]);
  const end = vi.fn(async () => {});
  const admin = vi
    .spyOn(Storage, "admin")
    .mockResolvedValue({ listFiles, end } as unknown as Awaited<
      ReturnType<typeof Storage.admin>
    >);
  localStorage.setItem(
    "XMTP_PENDING_ATTACHMENT_CLEANUP",
    JSON.stringify([unlistedPath, listedPath]),
  );
  try {
    const paths = await pendingAttachmentCleanupPaths();
    expect(paths).toEqual([listedPath]);
    for (const path of paths) await cleanSessionAttachments(path);
    await expect(listed.getDirectoryHandle(plaintext)).rejects.toMatchObject({
      name: "NotFoundError",
    });
    await expect(unlisted.getDirectoryHandle(plaintext)).resolves.toBeDefined();
    expect(listFiles).toHaveBeenCalledOnce();
    expect(end).toHaveBeenCalledOnce();
  } finally {
    admin.mockRestore();
    localStorage.removeItem("XMTP_PENDING_ATTACHMENT_CLEANUP");
    await root.removeEntry(`${listedPath}.attachments`, { recursive: true });
    await root.removeEntry(`${unlistedPath}.attachments`, { recursive: true });
  }
});

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

  it("keeps a local copy when OPFS reading fails", async () => {
    const root = await navigator.storage.getDirectory();
    const name = `web-chat-${crypto.randomUUID()}.bin`;
    await root.getFileHandle(name, { create: true });
    const remote = {
      url: "https://example.com/attachment",
    } as RemoteAttachment;
    const download = vi.fn(async () => ({
      path: name,
      mimeType: "text/plain",
    }));
    const deleteLocal = vi.fn(async () => root.removeEntry(name));
    const client = {
      attachments: { download, deleteLocal },
    } as unknown as Client;
    const getRoot = vi
      .spyOn(navigator.storage, "getDirectory")
      .mockResolvedValue(root);
    const read = vi
      .spyOn(root, "getFileHandle")
      .mockRejectedValueOnce(new Error("OPFS read failed"));
    try {
      await expect(downloadRemoteAttachment(client, remote)).rejects.toThrow(
        "OPFS read failed",
      );
      expect(download).toHaveBeenCalledExactlyOnceWith(remote);
      expect(deleteLocal).not.toHaveBeenCalled();
      await expect(root.getFileHandle(name)).resolves.toBeDefined();
    } finally {
      read.mockRestore();
      getRoot.mockRestore();
      await root.removeEntry(name).catch(() => {});
    }
  });
});

it("does not use an invalid cleanup journal path for OPFS removal", async () => {
  const key = "XMTP_PENDING_ATTACHMENT_CLEANUP";
  const malformed = "xmtp-sdk/test/../other/xmtp.db3";
  localStorage.setItem(key, JSON.stringify([malformed]));
  const getDirectory = vi.spyOn(navigator.storage, "getDirectory");
  try {
    expect(await pendingAttachmentCleanupPaths()).toEqual([]);
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
