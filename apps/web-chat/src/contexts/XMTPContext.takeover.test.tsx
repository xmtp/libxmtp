import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { APP_LOCK_ID_KEY } from "@/hooks/useAppLock";

import { XMTPProvider, useXMTP } from "./XMTPContext";

const mocks = vi.hoisted(() => {
  class StorageBusy extends Error {}
  return {
    StorageBusy,
    create: vi.fn(),
    canMessage: vi.fn().mockResolvedValue(new Map()),
    inboxIdFor: vi.fn().mockResolvedValue("registered-inbox-id"),
    listFiles: vi.fn().mockResolvedValue([]),
    reset: vi.fn(),
  };
});
vi.mock("@xmtp/browser-sdk", () => ({
  Client: {
    create: mocks.create,
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
  },
  Storage: {
    admin: async () => ({ listFiles: mocks.listFiles, end: async () => {} }),
  },
  initLogging: vi.fn(),
  XmtpError: { StorageBusy: mocks.StorageBusy },
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ reset: mocks.reset }),
}));

beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.clearAllMocks();
});

it("ends and cleans the old tab before the new owner connects", async () => {
  const deployment = `takeover-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const database = await backend.getDirectoryHandle(deployment, {
    create: true,
  });
  const inbox = await database.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const plaintext = "b".repeat(64);
  const staged = await attachments.getDirectoryHandle(".staged", {
    create: true,
  });
  await staged.getFileHandle("ciphertext", { create: true });

  const shutdown = Promise.withResolvers<void>();
  const firstEnd = vi.fn(async () => shutdown.promise);
  const firstClient = { storage: { path: async () => dbPath }, end: firstEnd };
  const secondClient = {
    storage: { path: async () => undefined },
    end: vi.fn(async () => {}),
  };
  mocks.create
    .mockResolvedValueOnce(firstClient)
    .mockResolvedValueOnce(secondClient);
  const signer = {
    identity: vi.fn(async () => ({ kind: "ethereum", identifier: "0x1234" })),
  };
  const first = renderHook(useXMTP, { wrapper: XMTPProvider });
  try {
    await act(async () => {
      await first.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: signer as never,
      });
    });
    await attachments.getDirectoryHandle(plaintext, { create: true });
    const oldLockId = localStorage.getItem(APP_LOCK_ID_KEY);

    const second = renderHook(useXMTP, { wrapper: XMTPProvider });
    try {
      expect(second.result.current.lockState).toBe("locked");
      act(() => {
        expect(second.result.current.acquireLock(true)).toBe(true);
      });
      const newLockId = localStorage.getItem(APP_LOCK_ID_KEY);
      expect(newLockId).not.toBe(oldLockId);
      act(() => {
        window.dispatchEvent(
          new StorageEvent("storage", {
            key: APP_LOCK_ID_KEY,
            newValue: newLockId,
            storageArea: localStorage,
          }),
        );
      });
      await waitFor(() => expect(firstEnd).toHaveBeenCalledOnce());
      await new Promise<void>((resolve) => setTimeout(resolve, 100));
      expect(await attachments.getDirectoryHandle(plaintext)).toBeDefined();
      expect(mocks.reset).not.toHaveBeenCalled();
      expect(second.result.current.lockState).toBe("active");

      await act(async () => {
        shutdown.resolve();
        await shutdown.promise;
      });
      await waitFor(() => expect(first.result.current.client).toBeUndefined());
      await expect(
        attachments.getDirectoryHandle(plaintext),
      ).rejects.toMatchObject({ name: "NotFoundError" });
      expect(await staged.getFileHandle("ciphertext")).toBeDefined();
      expect(mocks.reset).toHaveBeenCalledOnce();
      expect(localStorage.getItem(APP_LOCK_ID_KEY)).toBe(newLockId);

      await act(async () => {
        expect(
          await second.result.current.initialize({
            backendUrl: "https://example.com",
            env: "test",
            signer: signer as never,
          }),
        ).toBe(secondClient);
      });
      expect(second.result.current.client).toBe(secondClient);
      expect(second.result.current.lockState).toBe("active");
      await act(async () => {
        await second.result.current.disconnect();
      });
    } finally {
      shutdown.resolve();
      second.unmount();
    }
  } finally {
    first.unmount();
    await backend.removeEntry(deployment, { recursive: true });
  }
});
