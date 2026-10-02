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
    expect(await admin.poolCapacity()).toBeGreaterThanOrEqual(
      await admin.fileCount(),
    );
  } finally {
    await admin.end();
  }
});

test("public storage admin restores exact database bytes and preserves selected files", async () => {
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
    const second = await create(signer(), { storage: secondStorage });
    const firstPath = first.storagePath;
    const secondPath = second.storagePath;
    if (!firstPath || !secondPath) throw new Error("Persistent paths missing");
    expect(firstPath).not.toBe(secondPath);
    expect(await admin.listFiles()).toEqual(
      expect.arrayContaining([firstPath, secondPath]),
    );
    const group = await first.conversations.createGroup([]);
    const id = await group.sendText("restored content", { shouldPush: false });
    await expect(admin.exportDb(firstPath)).rejects.toBeInstanceOf(
      XmtpError.StorageBusy,
    );
    await first.end();
    await second.end();
    const exported = await admin.exportDb(firstPath);
    expect(exported.length).toBeGreaterThan(0);
    expect(await admin.deleteFile(firstPath)).toBe(true);
    expect(await admin.fileExists(firstPath)).toBe(false);
    expect(await admin.fileExists(secondPath)).toBe(true);
    await admin.importDb(firstPath, exported);
    expect(await admin.fileExists(firstPath)).toBe(true);
    expect(await admin.exportDb(firstPath)).toEqual(exported);
    const restored = await create(firstOwner, { storage: firstStorage });
    expect((await restored.conversations.getMessageById(id))?.content).toEqual({
      kind: "text",
      value: "restored content",
    });
    await restored.end();
    await admin.clearAll();
    expect(await admin.listFiles()).toEqual([]);
    expect(await admin.fileCount()).toBe(0);
  } finally {
    await admin.end();
  }
});
