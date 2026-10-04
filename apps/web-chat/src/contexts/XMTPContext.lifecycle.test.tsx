import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { generateInboxId, initPureWasm } from "@xmtp/browser-sdk/pure";
import { afterEach, beforeAll, expect, it, vi } from "vitest";

import { XMTPProvider, useXMTP } from "./XMTPContext";

const mocks = vi.hoisted(() => {
  class StorageBusy extends Error {}
  return {
    StorageBusy,
    create: vi.fn(),
    canMessage: vi
      .fn()
      .mockImplementation(([identity]) =>
        Promise.resolve(
          new Map([[`${identity.kind}:${identity.identifier}`, true]]),
        ),
      ),
    inboxIdFor: vi.fn().mockResolvedValue("registered-inbox-id"),
    listFiles: vi.fn().mockResolvedValue([]),
    endAdmin: vi.fn().mockResolvedValue(undefined),
    releaseLock: vi.fn(),
    acquireLock: vi.fn(() => true),
    ownsLock: vi.fn(() => true),
    reset: vi.fn(),
    lockLost: null as null | (() => void),
    signer: {},
  };
});
vi.mock("@xmtp/browser-sdk", async () => ({
  Client: {
    create: mocks.create,
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
  },
  generateInboxId: (await import("@xmtp/browser-sdk/pure")).generateInboxId,
  Storage: {
    admin: () =>
      Promise.resolve({ listFiles: mocks.listFiles, end: mocks.endAdmin }),
  },
  initLogging: vi.fn(),
  XmtpError: { StorageBusy: mocks.StorageBusy },
}));
beforeAll(async () => {
  await initPureWasm();
});
vi.mock("@/hooks/useAppLock", () => ({
  useAppLock: (onLockLost: () => void) => {
    mocks.lockLost = onLockLost;
    return {
      lockState: "available",
      acquireLock: mocks.acquireLock,
      releaseLock: mocks.releaseLock,
      ownsLock: mocks.ownsLock,
    };
  },
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ reset: mocks.reset }),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  mocks.lockLost = null;
  mocks.ownsLock.mockReturnValue(true);
});

it("opens the matching old Browser database when it exists", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  mocks.listFiles.mockResolvedValueOnce([
    "xmtp-test-someone-else.db3",
    "xmtp-test-registered-inbox-id.db3",
  ]);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(),
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: signer as never,
    });
  });

  expect(mocks.inboxIdFor).toHaveBeenCalledWith(identity, {
    url: "https://example.com",
    credentials: undefined,
    appVersion: "xmtp.chat/0",
  });
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: "xmtp-test-registered-inbox-id.db3",
          attachmentsDir: "xmtp-test-registered-inbox-id.db3.attachments",
        },
        label: "test",
      },
    }),
  );
  expect(mocks.endAdmin).toHaveBeenCalledTimes(1);
});

it("does not open another inbox's old Browser database", async () => {
  const signer = {
    identity: vi.fn().mockResolvedValue({
      identifier: "0x1234",
      kind: "ethereum",
    }),
  };
  mocks.listFiles.mockResolvedValueOnce(["xmtp-test-other-inbox.db3"]);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(),
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: signer as never,
    });
  });

  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: { location: "default", label: "test" },
    }),
  );
});

it("uses the version 7 nonce for an unregistered old database", async () => {
  const identity = {
    identifier: "0xabcdef0000000000000000000000000000000000",
    kind: "ethereum" as const,
  };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const legacyInboxId =
    "f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279";
  expect(generateInboxId(identity, 1n)).toBe(legacyInboxId);
  mocks.listFiles.mockResolvedValueOnce([`xmtp-test-${legacyInboxId}.db3`]);
  mocks.canMessage.mockResolvedValueOnce(
    new Map([[`${identity.kind}:${identity.identifier}`, false]]),
  );
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(),
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: signer as never,
    });
  });

  expect(mocks.inboxIdFor).not.toHaveBeenCalled();
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: `xmtp-test-${legacyInboxId}.db3`,
          attachmentsDir: `xmtp-test-${legacyInboxId}.db3.attachments`,
        },
        label: "test",
      },
    }),
  );
});

it("stops initialization when the old database check fails", async () => {
  mocks.listFiles.mockRejectedValueOnce(new Error("Storage check failed"));
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await expect(
      result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      }),
    ).rejects.toThrow("Storage check failed");
  });

  expect(mocks.create).not.toHaveBeenCalled();
  expect(mocks.endAdmin).toHaveBeenCalledTimes(1);
  expect(mocks.releaseLock).toHaveBeenCalledTimes(1);
});

it("shows a second-tab storage error and releases the app lock", async () => {
  mocks.create.mockRejectedValueOnce(new mocks.StorageBusy());
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
  await act(async () => {
    await expect(
      result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      }),
    ).rejects.toThrow("Another tab uses XMTP storage");
  });
  expect(result.current.client).toBeUndefined();
  expect(result.current.error?.message).toBe(
    "Another tab uses XMTP storage. Close that tab, then connect again.",
  );
  expect(mocks.releaseLock).toHaveBeenCalledTimes(1);
});

it("waits for SDK end before releasing the app lock", async () => {
  const held = Promise.withResolvers<void>();
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: () => held.promise,
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });
  let pending: Promise<void>;
  act(() => {
    pending = result.current.disconnect();
  });
  expect(mocks.releaseLock).not.toHaveBeenCalled();
  expect(result.current.client).toBeDefined();
  await act(async () => {
    held.resolve();
    await pending;
  });
  expect(mocks.releaseLock).toHaveBeenCalledTimes(1);
  expect(result.current.client).toBeUndefined();
});

it("removes old local attachment files when the user disconnects", async () => {
  const deployment = `disconnect-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  const end = vi.fn(async () => {});
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end,
  });
  try {
    const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await act(async () => {
      await result.current.disconnect();
    });
    expect(end).toHaveBeenCalledOnce();
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
    expect(mocks.releaseLock).toHaveBeenCalledOnce();
  } finally {
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("keeps the app lock until failed attachment cleanup is retried", async () => {
  const deployment = `retry-disconnect-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  const end = vi.fn(async () => {});
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end,
  });
  const getDirectory = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
    const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await act(async () => {
      await expect(result.current.disconnect()).rejects.toThrow("OPFS busy");
    });
    expect(result.current.client).toBeDefined();
    expect(mocks.releaseLock).not.toHaveBeenCalled();
    getDirectory.mockRestore();
    await act(async () => {
      await result.current.disconnect();
    });
    expect(end).toHaveBeenCalledOnce();
    expect(mocks.releaseLock).toHaveBeenCalledOnce();
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      { name: "NotFoundError" },
    );
  } finally {
    getDirectory.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("disconnects and reports a failed SDK end after lock loss", async () => {
  const end = vi.fn().mockRejectedValueOnce(new Error("Shutdown failed"));
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end,
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });
  act(() => mocks.lockLost?.());
  await waitFor(() => expect(result.current.client).toBeUndefined());
  expect(result.current.error?.message).toBe("Shutdown failed");
  expect(mocks.reset).toHaveBeenCalledTimes(1);
  expect(mocks.releaseLock).not.toHaveBeenCalled();
});

it("removes old local attachment files after lock loss even if storage path lookup fails", async () => {
  const deployment = `lock-loss-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  const end = vi.fn(async () => {});
  let pathLookupFails = false;
  mocks.create.mockResolvedValueOnce({
    storage: {
      path: async () => {
        if (pathLookupFails) throw new Error("Storage worker closed");
        return dbPath;
      },
    },
    end,
  });
  try {
    const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    pathLookupFails = true;
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(result.current.client).toBeUndefined());
    expect(end).toHaveBeenCalledOnce();
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
  } finally {
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("removes attachment files after lock loss when the journal write fails", async () => {
  const deployment = `journal-failure-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn(async () => {}),
  });
  const setItem = Storage.prototype.setItem;
  const storageSpy = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, key, value) {
      if (key === "XMTP_PENDING_ATTACHMENT_CLEANUP") {
        throw new Error("Cleanup journal unavailable");
      }
      return setItem.call(this, key, value);
    });
  try {
    const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(result.current.client).toBeUndefined());
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
  } finally {
    storageSpy.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("removes attachment files on the next start if lock-loss cleanup fails", async () => {
  const deployment = `retry-lock-loss-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  const end = vi.fn(async () => {});
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end,
  });
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(async () => {}),
  });
  const getDirectory = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
    const { result, unmount } = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(result.current.client).toBeUndefined());
    expect(result.current.error?.message).toBe("OPFS busy");
    unmount();
    getDirectory.mockRestore();
    const next = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await next.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      { name: "NotFoundError" },
    );
    expect(mocks.create).toHaveBeenCalledTimes(2);
  } finally {
    getDirectory.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("keeps a session retry when the journal and first cleanup fail", async () => {
  const deployment = `session-retry-${crypto.randomUUID()}`;
  const dbPath = `xmtp-sdk/test/${deployment}/inbox/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle("inbox", { create: true });
  await inbox.getDirectoryHandle("attachments", { create: true });
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn(async () => {}),
  });
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(async () => {}),
  });
  const setItem = Storage.prototype.setItem;
  const storageSpy = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, key, value) {
      if (this === localStorage && key === "XMTP_PENDING_ATTACHMENT_CLEANUP") {
        throw new Error("Cleanup journal unavailable");
      }
      return setItem.call(this, key, value);
    });
  const getDirectory = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
    const first = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await first.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(first.result.current.client).toBeUndefined());
    expect(
      sessionStorage.getItem("XMTP_PENDING_ATTACHMENT_CLEANUP_SESSION"),
    ).toContain(dbPath);
    first.unmount();
    getDirectory.mockRestore();
    const next = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await next.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await expect(inbox.getDirectoryHandle("attachments")).rejects.toMatchObject(
      {
        name: "NotFoundError",
      },
    );
    expect(
      sessionStorage.getItem("XMTP_PENDING_ATTACHMENT_CLEANUP_SESSION"),
    ).toBeNull();
  } finally {
    getDirectory.mockRestore();
    storageSpy.mockRestore();
    sessionStorage.removeItem("XMTP_PENDING_ATTACHMENT_CLEANUP_SESSION");
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("ends a client created after another tab takes the app lock", async () => {
  const created = Promise.withResolvers<{
    storage: { path: () => Promise<undefined> };
    end: () => Promise<void>;
  }>();
  const end = vi.fn().mockResolvedValue(undefined);
  mocks.create.mockReturnValueOnce(created.promise);
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  let pending!: Promise<unknown>;
  act(() => {
    pending = result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });
  await waitFor(() => expect(mocks.create).toHaveBeenCalledTimes(1));

  act(() => {
    mocks.ownsLock.mockReturnValue(false);
    mocks.lockLost?.();
  });
  await act(async () => {
    created.resolve({ storage: { path: async () => undefined }, end });
    await expect(pending).rejects.toThrow("App lock was lost");
  });

  expect(end).toHaveBeenCalledTimes(1);
  expect(result.current.client).toBeUndefined();
  expect(result.current.signer).toBeUndefined();
});

it("checks storage ownership when the lock-loss event is delayed", async () => {
  const created = Promise.withResolvers<{
    storage: { path: () => Promise<undefined> };
    end: () => Promise<void>;
  }>();
  const end = vi.fn().mockResolvedValue(undefined);
  mocks.create.mockReturnValueOnce(created.promise);
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  let pending!: Promise<unknown>;
  act(() => {
    pending = result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });
  await waitFor(() => expect(mocks.create).toHaveBeenCalledTimes(1));

  mocks.ownsLock.mockReturnValue(false);
  await act(async () => {
    created.resolve({ storage: { path: async () => undefined }, end });
    await expect(pending).rejects.toThrow("App lock was lost");
  });

  expect(end).toHaveBeenCalledTimes(1);
  expect(result.current.client).toBeUndefined();
});
