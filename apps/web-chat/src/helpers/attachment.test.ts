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
  retryPendingDatabaseDeletions,
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

it.each(["listed", "unlisted", "unavailable"])(
  "cleans only temporary files for a listed current database: %s",
  async (listing) => {
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
      const deployment = await backend.getDirectoryHandle(name, {
        create: true,
      });
      const inboxDir = await deployment.getDirectoryHandle(inbox, {
        create: true,
      });
      const directory = await inboxDir.getDirectoryHandle("attachments", {
        create: true,
      });
      await directory.getDirectoryHandle(plaintext, { create: true });
      await directory.getDirectoryHandle(".tmp", { create: true });
      await directory.getDirectoryHandle(".staged", { create: true });
      return directory;
    };
    const selectedAttachments = await attachments(selected);
    const forgedAttachments = await attachments(forged);
    const dbPath = `xmtp-sdk/${label}/${selected}/${inbox}/xmtp.db3`;
    const listFiles = vi.fn(async () => {
      if (listing === "unavailable") throw new Error("listing failed");
      return listing === "listed" ? [`/${dbPath}`] : [];
    });
    const end = vi.fn(async () => {});
    const admin = vi
      .spyOn(Storage, "admin")
      .mockResolvedValue({ listFiles, end } as unknown as Awaited<
        ReturnType<typeof Storage.admin>
      >);
    try {
      await cleanStoredSessionAttachments();
      for (const name of [plaintext, ".tmp"]) {
        if (listing === "listed" && name === ".tmp") {
          await expect(
            selectedAttachments.getDirectoryHandle(name),
          ).rejects.toMatchObject({ name: "NotFoundError" });
        } else {
          await expect(
            selectedAttachments.getDirectoryHandle(name),
          ).resolves.toBeDefined();
        }
      }
      await expect(
        selectedAttachments.getDirectoryHandle(".staged"),
      ).resolves.toBeDefined();
      await expect(
        forgedAttachments.getDirectoryHandle(plaintext),
      ).resolves.toBeDefined();
      expect(listFiles).toHaveBeenCalledOnce();
      expect(end).toHaveBeenCalledOnce();
    } finally {
      admin.mockRestore();
      await sdk.removeEntry(label, { recursive: true });
    }
  },
);

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

it("preserves completed legacy attachments for a listed database", async () => {
  const dbPath = `xmtp-${crypto.randomUUID()}-${"a".repeat(64)}.db3`;
  const plaintext = "b".repeat(64);
  const root = await navigator.storage.getDirectory();
  await root.getFileHandle(dbPath, { create: true });
  const attachments = await root.getDirectoryHandle(`${dbPath}.attachments`, {
    create: true,
  });
  await attachments.getDirectoryHandle(plaintext, { create: true });
  await attachments.getDirectoryHandle(".tmp", { create: true });
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
    ).resolves.toBeDefined();
    await expect(attachments.getDirectoryHandle(".tmp")).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    admin.mockRestore();
    await root.removeEntry(`${dbPath}.attachments`, { recursive: true });
    await root.removeEntry(dbPath);
  }
});

it.each(["listed", "unlisted", "unavailable"])(
  "admits only recorded and listed current cleanup journal paths: %s",
  async (listing) => {
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
    const listFiles = vi.fn(async () => {
      if (listing === "unavailable") throw new Error("listing failed");
      return listing === "listed" ? [`/${selectedPath}`, forgedPath] : [];
    });
    const end = vi.fn(async () => {});
    const admin = vi
      .spyOn(Storage, "admin")
      .mockResolvedValue({ listFiles, end } as unknown as Awaited<
        ReturnType<typeof Storage.admin>
      >);
    try {
      expect(await pendingAttachmentCleanupPaths()).toEqual(
        listing === "listed" ? [selectedPath] : [],
      );
      expect(listFiles).toHaveBeenCalledOnce();
      expect(end).toHaveBeenCalledOnce();
    } finally {
      admin.mockRestore();
      localStorage.removeItem("XMTP_PENDING_ATTACHMENT_CLEANUP");
      await sdk.removeEntry(label, { recursive: true });
    }
  },
);

it("replays legacy cleanup only for a listed database", async () => {
  const listedPath = `xmtp-${crypto.randomUUID()}-${"a".repeat(64)}.db3`;
  const unlistedPath = `xmtp-${crypto.randomUUID()}-${"b".repeat(64)}.db3`;
  const plaintext = ".tmp";
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
      expect(deleteLocal).not.toHaveBeenCalled();
      expect(
        await (await (await directory.getFileHandle(name)).getFile()).text(),
      ).toBe("hello from xmtp.chat");
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

it("keeps downloaded bytes without invoking local deletion", async () => {
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
    expect(deleteLocal).not.toHaveBeenCalled();
  } finally {
    await directory.removeEntry(name).catch(() => {});
  }
});

it.each(["listed", "unlisted", "unavailable"])(
  "replays database deletion only with listed ownership: %s",
  async (listing) => {
    const label = `delete-${crypto.randomUUID()}`;
    const identifier = "selected-deployment";
    const deployment = await deploymentComponent(identifier);
    const inbox = "a".repeat(64);
    const dbPath = `xmtp-sdk/${label}/${deployment}/${inbox}/xmtp.db3`;
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
    const folder = await backend.getDirectoryHandle(deployment, {
      create: true,
    });
    const directory = await folder.getDirectoryHandle(inbox, { create: true });
    const attachments = await directory.getDirectoryHandle("attachments", {
      create: true,
    });
    const file = await attachments.getFileHandle("retained", { create: true });
    const bytes = await file.createWritable();
    await bytes.write("keep until ownership is proved");
    await bytes.close();
    const listFiles = vi.fn(async () => {
      if (listing === "unavailable") throw new Error("listing failed");
      return listing === "listed" ? [`/${dbPath}`] : [];
    });
    const deleteFile = vi.fn(async () => {});
    const end = vi.fn(async () => {});
    const admin = vi
      .spyOn(Storage, "admin")
      .mockResolvedValue({ listFiles, deleteFile, end } as unknown as Awaited<
        ReturnType<typeof Storage.admin>
      >);
    localStorage.setItem(
      "XMTP_PENDING_DATABASE_DELETION",
      JSON.stringify([dbPath]),
    );
    try {
      await retryPendingDatabaseDeletions();
      expect(listFiles).toHaveBeenCalledOnce();
      expect(end).toHaveBeenCalledOnce();
      if (listing === "listed") {
        expect(deleteFile).toHaveBeenCalledExactlyOnceWith(dbPath);
        expect(
          localStorage.getItem("XMTP_PENDING_DATABASE_DELETION"),
        ).toBeNull();
        await expect(
          directory.getDirectoryHandle("attachments"),
        ).rejects.toMatchObject({ name: "NotFoundError" });
      } else {
        expect(deleteFile).not.toHaveBeenCalled();
        expect(
          JSON.parse(localStorage.getItem("XMTP_PENDING_DATABASE_DELETION")!),
        ).toEqual([dbPath]);
        expect(await (await file.getFile()).text()).toBe(
          "keep until ownership is proved",
        );
      }
    } finally {
      admin.mockRestore();
      localStorage.removeItem("XMTP_PENDING_DATABASE_DELETION");
      await sdk.removeEntry(label, { recursive: true });
    }
  },
);
