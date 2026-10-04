import { Client, Storage, XmtpError } from "@xmtp/browser-sdk";
import { generateInboxId, initPureWasm } from "@xmtp/browser-sdk/pure";
import { expect, test } from "vitest";

import { resolveLegacyStorage } from "../dist/typescript-wasm/runtime/public/host.js";
import { create, options, signer } from "./helpers";

test("default storage reopens a version 7 OPFS database", async () => {
  const owner = signer();
  const probe = await create(owner);
  const inboxId = probe.inboxId;
  await probe.end();
  const oldPath = `xmtp-default-${inboxId}.db3`;
  let legacy: Client | undefined;
  let upgraded: Client | undefined;
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
    expect(upgraded.storagePath).toBe(oldPath);
    expect(
      (await upgraded.conversations.getMessageById(messageId))?.content,
    ).toEqual({
      kind: "text",
      value: "before upgrade",
    });
  } finally {
    await Promise.allSettled([legacy?.end(), upgraded?.end()]);
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

test("default storage resolves a version 7 nonce-one database for offline build", async () => {
  await initPureWasm();
  const owner = signer();
  const identity = await owner.identity();
  const inboxId = generateInboxId(identity, 1n);
  const oldPath = `xmtp-default-${inboxId}.db3`;
  let legacy: Client | undefined;
  let upgraded: Client | undefined;
  try {
    legacy = await create(owner, {
      registration: { nonce: 1n },
      storage: {
        location: {
          dbPath: oldPath,
          attachmentsDir: `${oldPath}.attachments`,
        },
      },
    });
    expect(legacy.inboxId).toBe(inboxId);
    const group = await legacy.conversations.createGroup([]);
    const messageId = await group.sendText("before offline upgrade", {
      shouldPush: false,
    });
    await legacy.end();

    const defaultOptions = {
      ...options,
      allowOffline: true,
      storage: { location: "default" as const },
    };
    const resolved = await resolveLegacyStorage(defaultOptions, () =>
      Promise.resolve(identity),
    );
    expect(resolved.storage.location).toEqual({
      dbPath: oldPath,
      attachmentsDir: `${oldPath}.attachments`,
    });
    expect(resolved.registration?.nonce).toBe(1n);
    upgraded = await Client.build(identity, defaultOptions, inboxId);
    expect(upgraded.storagePath).toBe(oldPath);
    expect(
      (await upgraded.conversations.getMessageById(messageId))?.content,
    ).toEqual({ kind: "text", value: "before offline upgrade" });
  } finally {
    await Promise.allSettled([legacy?.end(), upgraded?.end()]);
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

test("default storage rejects a legacy and current database for one inbox", async () => {
  const owner = signer();
  const probe = await create(owner);
  const inboxId = probe.inboxId;
  await probe.end();
  const oldPath = `xmtp-default-${inboxId}.db3`;
  let legacy: Client | undefined;
  let current: Client | undefined;
  let currentPath: string | undefined;
  try {
    legacy = await create(owner, {
      storage: {
        location: {
          dbPath: oldPath,
          attachmentsDir: `${oldPath}.attachments`,
        },
      },
    });
    await legacy.end();
    current = await create(owner, {
      storage: { location: { directory: "xmtp-sdk" } },
    });
    currentPath = current.storagePath;
    await current.end();

    await expect(
      create(owner, { storage: { location: "default" } }),
    ).rejects.toBeInstanceOf(XmtpError.StorageLocation);
  } finally {
    await Promise.allSettled([legacy?.end(), current?.end()]);
    const admin = await Storage.admin();
    try {
      await admin.deleteFile(oldPath);
      if (currentPath) await admin.deleteFile(currentPath);
    } finally {
      await admin.end();
    }
    const root = await navigator.storage.getDirectory();
    await root
      .removeEntry(`${oldPath}.attachments`, { recursive: true })
      .catch(() => {});
  }
});
