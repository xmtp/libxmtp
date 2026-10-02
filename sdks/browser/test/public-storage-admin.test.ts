import { Storage, XmtpError } from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

import { create, signer } from "./helpers";

test("public storage admin keeps absent-file and invalid-import failures", async () => {
  const admin = await Storage.admin();
  try {
    const absent = `absent-${crypto.randomUUID()}.db`;
    expect(await admin.fileExists(absent)).toBe(false);
    expect(await admin.deleteFile(absent)).toBe(false);
    await expect(admin.exportDb(absent)).rejects.toThrow();
    await expect(
      admin.importDb(absent, new Uint8Array([1, 2, 3, 4, 5])),
    ).rejects.toThrow();
    expect(await admin.fileExists(absent)).toBe(false);
    expect(await admin.fileCount()).toBe((await admin.listFiles()).length);
    expect(await admin.poolCapacity()).toBeGreaterThanOrEqual(6);
  } finally {
    await admin.end();
  }
});

test("public storage admin restores database content and preserves selected files", async () => {
  const admin = await Storage.admin();
  const firstOwner = signer();
  const firstStorage = {
    location: { directory: `admin-first-${crypto.randomUUID()}` },
  } as const;
  const secondStorage = {
    location: { directory: `admin-second-${crypto.randomUUID()}` },
  } as const;
  try {
    await admin.clearAll();
    expect(await admin.listFiles()).toEqual([]);
    expect(await admin.fileCount()).toBe(0);
    const first = await create(firstOwner, { storage: firstStorage });
    const firstPath = first.storagePath;
    if (!firstPath) throw new Error("Persistent path missing");
    expect(await admin.listFiles()).toEqual([firstPath]);
    expect(await admin.fileCount()).toBe(1);
    expect(await admin.fileExists(firstPath)).toBe(true);
    const second = await create(signer(), { storage: secondStorage });
    const secondPath = second.storagePath;
    if (!secondPath) throw new Error("Persistent path missing");
    expect(firstPath).not.toBe(secondPath);
    expect((await admin.listFiles()).sort()).toEqual(
      [firstPath, secondPath].sort(),
    );
    expect(await admin.fileCount()).toBe(2);
    expect(await admin.fileExists(secondPath)).toBe(true);
    const group = await first.conversations.createGroup([]);
    const id = await group.sendText("restored content", { shouldPush: false });
    await expect(admin.exportDb(firstPath)).rejects.toBeInstanceOf(
      XmtpError.StorageBusy,
    );
    await first.end();
    await second.end();
    const exported = await admin.exportDb(firstPath);
    expect(exported.length).toBeGreaterThan(0);
    expect(new TextDecoder().decode(exported.slice(0, 16))).toBe(
      "SQLite format 3\0",
    );

    const copyPath = `admin-copy-${crypto.randomUUID()}.db`;
    await admin.importDb(copyPath, exported);
    expect((await admin.listFiles()).sort()).toEqual(
      [firstPath, secondPath, copyPath].sort(),
    );
    expect(await admin.fileCount()).toBe(3);
    expect(await admin.fileExists(copyPath)).toBe(true);
    const copied = await admin.exportDb(copyPath);
    // Import rotates the database stream identity.
    expect(copied.length).toBe(exported.length);
    expect(copied).not.toEqual(exported);
    expect(await admin.exportDb(firstPath)).toEqual(exported);
    expect(await admin.deleteFile(copyPath)).toBe(true);
    expect(await admin.fileExists(copyPath)).toBe(false);
    expect(await admin.fileCount()).toBe(2);

    expect(await admin.deleteFile(firstPath)).toBe(true);
    expect(await admin.fileExists(firstPath)).toBe(false);
    expect(await admin.fileExists(secondPath)).toBe(true);
    expect(await admin.listFiles()).toEqual([secondPath]);
    expect(await admin.fileCount()).toBe(1);

    const replacement = await create(signer(), {
      storage: {
        location: {
          dbPath: firstPath,
          attachmentsDir: `${firstPath}-replacement-attachments`,
        },
      },
    });
    expect(replacement.storagePath).toBe(firstPath);
    expect(replacement.inboxId).not.toBe(first.inboxId);
    const replacementGroup = await replacement.conversations.createGroup([]);
    const replacementId = await replacementGroup.sendText("replacement content", {
      shouldPush: false,
    });
    await replacement.end();
    expect(await admin.exportDb(firstPath)).not.toEqual(exported);
    expect(await admin.deleteFile(firstPath)).toBe(true);
    await admin.importDb(firstPath, exported);
    expect(await admin.fileExists(firstPath)).toBe(true);
    expect((await admin.listFiles()).sort()).toEqual(
      [firstPath, secondPath].sort(),
    );
    expect(await admin.fileCount()).toBe(2);
    const restoredBytes = await admin.exportDb(firstPath);
    expect(restoredBytes.length).toBe(exported.length);
    expect(restoredBytes).not.toEqual(exported);
    const restored = await create(firstOwner, { storage: firstStorage });
    expect(restored.inboxId).toBe(first.inboxId);
    expect((await restored.conversations.getMessageById(id))?.content).toEqual({
      kind: "text",
      value: "restored content",
    });
    expect(
      await restored.conversations.getMessageById(replacementId),
    ).toBeUndefined();
    await restored.end();
    await admin.clearAll();
    expect(await admin.listFiles()).toEqual([]);
    expect(await admin.fileCount()).toBe(0);
  } finally {
    await admin.end();
  }
});
