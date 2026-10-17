import { Storage, type Client } from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

import { create, signer } from "./helpers";

test("default storage leaves a version 7 OPFS database for explicit reopen", async () => {
  const owner = signer();
  const probe = await create(owner);
  const inboxId = probe.inboxId;
  await probe.end();
  const oldPath = `xmtp-default-${inboxId}.db3`;
  let legacy: Client | undefined;
  let upgraded: Client | undefined;
  let reopened: Client | undefined;
  try {
    legacy = await create(owner, {
      storage: {
        location: {
          dbPath: oldPath,
          attachmentsDir: `${oldPath}.attachments`,
        },
      },
    });
    const group = await legacy.conversations.createGroup([]);
    const messageId = await group.sendText("before upgrade", {
      shouldPush: false,
    });
    await legacy.end();

    upgraded = await create(owner, { storage: { location: "default" } });
    expect(upgraded.storagePath).not.toBe(oldPath);
    reopened = await create(owner, {
      storage: {
        location: {
          dbPath: oldPath,
          attachmentsDir: `${oldPath}.attachments`,
        },
      },
    });
    expect(
      (await reopened.conversations.getMessageById(messageId))?.content,
    ).toEqual({
      kind: "text",
      value: "before upgrade",
    });
  } finally {
    await Promise.allSettled([legacy?.end(), upgraded?.end(), reopened?.end()]);
    const admin = await Storage.admin();
    try {
      for (const file of await admin.listFiles()) {
        const path = file.replace(/^\/+/, "");
        if (path === oldPath || path.endsWith(`/${inboxId}/xmtp.db3`)) {
          await admin.deleteFile(file);
        }
      }
    } finally {
      await admin.end();
    }
    const root = await navigator.storage.getDirectory();
    await root
      .removeEntry(`${oldPath}.attachments`, { recursive: true })
      .catch(() => {});
  }
});
