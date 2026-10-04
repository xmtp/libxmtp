import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { deploymentComponent } from "@/helpers/attachment";

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
    fetchServerConfiguration: vi
      .fn()
      .mockResolvedValue({ identifier: "selected-deployment" }),
    listFiles: vi.fn().mockResolvedValue([]),
    endAdmin: vi.fn().mockResolvedValue(undefined),
    releaseLock: vi.fn(),
    acquireLock: vi.fn(() => true),
    ownsLock: vi.fn(() => true),
    reset: vi.fn(),
    lockLost: null as null | (() => void),
    pageHide: null as null | (() => Promise<void> | void),
    signer: {},
  };
});
vi.mock("@xmtp/browser-sdk", async () => ({
  Client: {
    create: mocks.create,
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
    fetchServerConfiguration: mocks.fetchServerConfiguration,
  },
  Storage: {
    admin: () =>
      Promise.resolve({ listFiles: mocks.listFiles, end: mocks.endAdmin }),
  },
  initLogging: vi.fn(),
  XmtpError: { StorageBusy: mocks.StorageBusy },
}));
vi.mock("@/hooks/useAppLock", () => ({
  useAppLock: (
    onLockLost: () => void,
    onPageHide: () => Promise<void> | void,
  ) => {
    mocks.lockLost = onLockLost;
    mocks.pageHide = onPageHide;
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
  mocks.pageHide = null;
  mocks.ownsLock.mockReturnValue(true);
});

it("opens the app's matching old Browser database", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId = "a".repeat(64);
  mocks.inboxIdFor.mockResolvedValueOnce(inboxId);
  mocks.listFiles.mockResolvedValueOnce([
    "xmtp-test-someone-else.db3",
    `xmtp-test-${inboxId}.db3`,
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
  expect(mocks.fetchServerConfiguration).not.toHaveBeenCalled();
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: `xmtp-test-${inboxId}.db3`,
          attachmentsDir: `xmtp-test-${inboxId}.db3.attachments`,
        },
        label: "test",
      },
      registration: { nonce: 1n },
    }),
  );
  expect(mocks.endAdmin).toHaveBeenCalledTimes(1);
});

it("opens an uppercase old Browser database by its stored path", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId = "abcdef0123456789".repeat(4);
  const storedPath = `xmtp-test-${inboxId.toUpperCase()}.db3`;
  mocks.inboxIdFor.mockResolvedValueOnce(inboxId);
  mocks.listFiles.mockResolvedValueOnce([storedPath]);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => storedPath },
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
      storage: {
        location: {
          dbPath: storedPath,
          attachmentsDir: `${storedPath}.attachments`,
        },
        label: "test",
      },
      registration: { nonce: 1n },
    }),
  );
});

it("stops when old and current app databases match one inbox", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId = "b".repeat(64);
  const deploymentHash = Array.from(
    new Uint8Array(
      await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode("selected-deployment"),
      ),
    ),
  )
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  mocks.inboxIdFor.mockResolvedValueOnce(inboxId);
  mocks.listFiles.mockResolvedValueOnce([
    `xmtp-test-${inboxId}.db3`,
    `xmtp-sdk/test/selected-deployment-${deploymentHash}/${inboxId}/xmtp.db3`,
  ]);
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await expect(
      result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: signer as never,
      }),
    ).rejects.toThrow("Both old and current databases match this inbox");
  });
  expect(mocks.create).not.toHaveBeenCalled();
});

it("opens an old database when the current path belongs to another deployment", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId = "b".repeat(64);
  const otherHash = Array.from(
    new Uint8Array(
      await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode("other-deployment"),
      ),
    ),
  )
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  mocks.inboxIdFor.mockResolvedValueOnce(inboxId);
  mocks.listFiles.mockResolvedValueOnce([
    `xmtp-test-${inboxId}.db3`,
    `xmtp-sdk/test/other-deployment-${otherHash}/${inboxId}/xmtp.db3`,
  ]);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(),
  });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com/a",
      env: "test",
      signer: signer as never,
    });
  });

  expect(mocks.fetchServerConfiguration).toHaveBeenCalledWith({
    url: "https://example.com/a",
    credentials: undefined,
    appVersion: "xmtp.chat/0",
  });
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: `xmtp-test-${inboxId}.db3`,
          attachmentsDir: `xmtp-test-${inboxId}.db3.attachments`,
        },
        label: "test",
      },
    }),
  );
});

it("ignores a malformed current path beside a matching old database", async () => {
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId = "b".repeat(64);
  mocks.inboxIdFor.mockResolvedValueOnce(inboxId);
  mocks.listFiles.mockResolvedValueOnce([
    `xmtp-test-${inboxId}.db3`,
    `xmtp-sdk/test/deployment/${inboxId}/xmtp.db3`,
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
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: `xmtp-test-${inboxId}.db3`,
          attachmentsDir: `xmtp-test-${inboxId}.db3.attachments`,
        },
        label: "test",
      },
    }),
  );
});

it("opens an unregistered version 7 database with its nonce-one inbox", async () => {
  const identity = {
    identifier: "0xabcdef0000000000000000000000000000000000",
    kind: "ethereum" as const,
  };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  const inboxId =
    "f020cf771dabaf2610250b5f00076215a8f1da8649ba46cf5ba2d00df6ce5279";
  mocks.listFiles.mockResolvedValueOnce([`xmtp-test-${inboxId}.db3`]);
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
          dbPath: `xmtp-test-${inboxId}.db3`,
          attachmentsDir: `xmtp-test-${inboxId}.db3.attachments`,
        },
        label: "test",
      },
      registration: { nonce: 1n },
    }),
  );
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

it("ends an injected client when its storage path lookup fails", async () => {
  const end = vi.fn(async () => {});
  const injected = {
    storage: { path: vi.fn().mockRejectedValue(new Error("Storage closed")) },
    end,
  };
  const { result } = renderHook(useXMTP, {
    wrapper: ({ children }) => (
      <XMTPProvider client={injected as never}>{children}</XMTPProvider>
    ),
  });
  await act(async () => {
    await result.current.disconnect();
  });
  expect(end).toHaveBeenCalledOnce();
  expect(result.current.client).toBeUndefined();
  expect(mocks.reset).toHaveBeenCalledOnce();
  expect(mocks.releaseLock).toHaveBeenCalledOnce();
});

it("ends an injected client on pagehide when its storage path lookup fails", async () => {
  const end = vi.fn(async () => {});
  const path = vi.fn().mockRejectedValue(new Error("Storage closed"));
  const injected = { storage: { path }, end };
  renderHook(useXMTP, {
    wrapper: ({ children }) => (
      <XMTPProvider client={injected as never}>{children}</XMTPProvider>
    ),
  });

  await act(async () => {
    await mocks.pageHide?.();
  });

  expect(path).toHaveBeenCalledOnce();
  expect(end).toHaveBeenCalledOnce();
});

it("removes local plaintext but keeps staged ciphertext on disconnect", async () => {
  const deployment = `disconnect-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "a".repeat(64);
  const staged = await attachments.getDirectoryHandle(".staged", {
    create: true,
  });
  const digest = "b".repeat(64);
  await staged.getFileHandle(digest, { create: true });
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
    await attachments.getDirectoryHandle(key, { create: true });
    await act(async () => {
      await result.current.disconnect();
    });
    expect(end).toHaveBeenCalledOnce();
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
    expect(await staged.getFileHandle(digest)).toBeDefined();
    expect(mocks.releaseLock).toHaveBeenCalledOnce();
  } finally {
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("records pagehide cleanup and removes plaintext before a new account starts", async () => {
  const identifier = `pagehide-${crypto.randomUUID()}`;
  const deployment = await deploymentComponent(identifier);
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com": identifier },
    }),
  );
  await writer.close();
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "f".repeat(64);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn(async () => {}),
  });
  const journalKey = "XMTP_PENDING_ATTACHMENT_CLEANUP";
  try {
    const first = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await first.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await attachments.getDirectoryHandle(key, { create: true });
    const getDirectory = vi
      .spyOn(navigator.storage, "getDirectory")
      .mockRejectedValueOnce(new Error("OPFS busy"));
    const hidden = mocks.pageHide?.();
    await expect(hidden).rejects.toThrow("OPFS busy");
    expect(JSON.parse(localStorage.getItem(journalKey)!)).toContain(dbPath);
    expect(mocks.releaseLock).not.toHaveBeenCalled();
    expect(await attachments.getDirectoryHandle(key)).toBeDefined();
    getDirectory.mockRestore();
    first.unmount();

    mocks.create.mockResolvedValueOnce({
      storage: { path: async () => undefined },
      end: vi.fn(async () => {}),
    });
    const second = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await second.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
    expect(JSON.parse(localStorage.getItem(journalKey) ?? "[]")).not.toContain(
      dbPath,
    );
  } finally {
    localStorage.removeItem(journalKey);
    await backend.removeEntry(deployment, { recursive: true });
    await backend.removeEntry("deployments.json");
  }
});

it("removes pagehide plaintext when the cleanup journal write fails", async () => {
  const deployment = `pagehide-failure-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "d".repeat(64);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn(async () => {}),
  });
  const setItem = Storage.prototype.setItem;
  const storageSpy = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, name, value) {
      if (name === "XMTP_PENDING_ATTACHMENT_CLEANUP") {
        throw new Error("Cleanup journal unavailable");
      }
      return setItem.call(this, name, value);
    });
  try {
    const { result, unmount } = renderHook(useXMTP, {
      wrapper: XMTPProvider,
    });
    await act(async () => {
      await result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: mocks.signer as never,
      });
    });
    await attachments.getDirectoryHandle(key, { create: true });
    await act(async () => {
      await mocks.pageHide?.();
    });
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
    unmount();
    mocks.create.mockResolvedValueOnce({
      storage: { path: async () => undefined },
      end: vi.fn(async () => {}),
    });
    const otherSigner = {};
    const next = renderHook(useXMTP, { wrapper: XMTPProvider });
    await act(async () => {
      await next.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: otherSigner as never,
      });
    });
    expect(mocks.create).toHaveBeenCalledWith(otherSigner, expect.anything());
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    storageSpy.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("waits for in-flight work before pagehide removes plaintext", async () => {
  const deployment = `pagehide-flight-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
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
  const plaintext = "d".repeat(64);
  const finishDownload = Promise.withResolvers<void>();
  const end = vi.fn(async () => {
    await finishDownload.promise;
    await attachments.getDirectoryHandle(plaintext, { create: true });
  });
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
    const hidden = mocks.pageHide?.();
    expect(end).toHaveBeenCalledOnce();
    await expect(
      attachments.getDirectoryHandle(plaintext),
    ).rejects.toMatchObject({
      name: "NotFoundError",
    });
    expect(localStorage.getItem("XMTP_PENDING_ATTACHMENT_CLEANUP")).toBeNull();
    finishDownload.resolve();
    await act(async () => {
      await hidden;
    });
    await expect(
      attachments.getDirectoryHandle(plaintext),
    ).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    finishDownload.resolve();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("keeps pagehide cleanup incomplete when client shutdown fails", async () => {
  const deployment = `pagehide-end-failure-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
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
  const plaintext = "e".repeat(64);
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn().mockRejectedValue(new Error("Shutdown failed")),
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
    await attachments.getDirectoryHandle(plaintext, { create: true });
    await expect(mocks.pageHide?.()).rejects.toThrow("Shutdown failed");
    expect(await attachments.getDirectoryHandle(plaintext)).toBeDefined();
    expect(localStorage.getItem("XMTP_PENDING_ATTACHMENT_CLEANUP")).toBeNull();
  } finally {
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("cleans orphan plaintext before another inbox starts after both pagehide safeguards fail", async () => {
  const identifier = `orphan-${crypto.randomUUID()}`;
  const deployment = await deploymentComponent(identifier);
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const legacyDbPath = `xmtp-test-${"b".repeat(64)}.db3`;
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end: vi.fn(async () => {}),
  });
  const first = renderHook(useXMTP, { wrapper: XMTPProvider });
  await act(async () => {
    await first.result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });

  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com": identifier },
    }),
  );
  await writer.close();
  const database = await backend.getDirectoryHandle(deployment, {
    create: true,
  });
  const inbox = await database.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const plaintext = "c".repeat(64);
  await attachments.getDirectoryHandle(plaintext, { create: true });
  const staged = await attachments.getDirectoryHandle(".staged", {
    create: true,
  });
  await staged.getFileHandle("ciphertext", { create: true });
  await inbox.getFileHandle("xmtp.db3", { create: true });
  const legacy = await root.getDirectoryHandle(`${legacyDbPath}.attachments`, {
    create: true,
  });
  await legacy.getDirectoryHandle(plaintext, { create: true });
  const unrelatedName = `unrelated-${crypto.randomUUID()}`;
  const unrelated = await root.getDirectoryHandle(unrelatedName, {
    create: true,
  });
  await unrelated.getDirectoryHandle(plaintext, { create: true });

  const setItem = Storage.prototype.setItem;
  const storageSpy = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(function (this: Storage, name, value) {
      if (name === "XMTP_PENDING_ATTACHMENT_CLEANUP") {
        throw new Error("Cleanup journal unavailable");
      }
      return setItem.call(this, name, value);
    });
  const opfsSpy = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
    await expect(mocks.pageHide?.()).rejects.toThrow("OPFS busy");
    expect(localStorage.getItem("XMTP_PENDING_ATTACHMENT_CLEANUP")).toBeNull();
    expect(await attachments.getDirectoryHandle(plaintext)).toBeDefined();
    storageSpy.mockRestore();
    opfsSpy.mockRestore();
    first.unmount();

    const otherSigner = {};
    mocks.create.mockImplementationOnce(async () => {
      await expect(
        attachments.getDirectoryHandle(plaintext),
      ).rejects.toMatchObject({
        name: "NotFoundError",
      });
      await expect(legacy.getDirectoryHandle(plaintext)).rejects.toMatchObject({
        name: "NotFoundError",
      });
      return {
        storage: { path: async () => undefined },
        end: vi.fn(async () => {}),
      };
    });
    const second = renderHook(useXMTP, { wrapper: XMTPProvider });
    const stillBusy = vi
      .spyOn(navigator.storage, "getDirectory")
      .mockRejectedValueOnce(new Error("OPFS still busy"));
    await act(async () => {
      await expect(
        second.result.current.initialize({
          backendUrl: "https://example.com",
          env: "test",
          signer: otherSigner as never,
        }),
      ).rejects.toThrow("OPFS still busy");
    });
    expect(mocks.create).toHaveBeenCalledTimes(1);
    stillBusy.mockRestore();

    await act(async () => {
      await second.result.current.initialize({
        backendUrl: "https://example.com",
        env: "test",
        signer: otherSigner as never,
      });
    });
    expect(mocks.create).toHaveBeenCalledTimes(2);
    expect(await staged.getFileHandle("ciphertext")).toBeDefined();
    expect(await inbox.getFileHandle("xmtp.db3")).toBeDefined();
    expect(await unrelated.getDirectoryHandle(plaintext)).toBeDefined();
  } finally {
    storageSpy.mockRestore();
    opfsSpy.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
    await backend.removeEntry("deployments.json");
    await root.removeEntry(`${legacyDbPath}.attachments`, { recursive: true });
    await root.removeEntry(unrelatedName, { recursive: true });
  }
});

it("keeps the app lock until failed attachment cleanup is retried", async () => {
  const deployment = `retry-disconnect-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "c".repeat(64);
  const end = vi.fn(async () => {});
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
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
  await attachments.getDirectoryHandle(key, { create: true });
  const getDirectory = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
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
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
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
  const deployment = `lock-loss-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "d".repeat(64);
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
    await attachments.getDirectoryHandle(key, { create: true });
    pathLookupFails = true;
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(result.current.client).toBeUndefined());
    expect(end).toHaveBeenCalledOnce();
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("removes attachment files after lock loss when the journal write fails", async () => {
  const deployment = `journal-failure-${crypto.randomUUID().replaceAll("-", "").repeat(2)}`;
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "e".repeat(64);
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
    await attachments.getDirectoryHandle(key, { create: true });
    act(() => mocks.lockLost?.());
    await waitFor(() => expect(result.current.client).toBeUndefined());
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
  } finally {
    storageSpy.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
  }
});

it("removes attachment files on the next start if lock-loss cleanup fails", async () => {
  const identifier = `retry-lock-loss-${crypto.randomUUID()}`;
  const deployment = await deploymentComponent(identifier);
  const inboxId = "a".repeat(64);
  const dbPath = `xmtp-sdk/test/${deployment}/${inboxId}/xmtp.db3`;
  const root = await navigator.storage.getDirectory();
  const sdk = await root.getDirectoryHandle("xmtp-sdk", { create: true });
  const backend = await sdk.getDirectoryHandle("test", { create: true });
  const record = await backend.getFileHandle("deployments.json", {
    create: true,
  });
  const writer = await record.createWritable();
  await writer.write(
    JSON.stringify({
      version: 1,
      deployments: { "https://example.com": identifier },
    }),
  );
  await writer.close();
  const path = await backend.getDirectoryHandle(deployment, { create: true });
  const inbox = await path.getDirectoryHandle(inboxId, { create: true });
  const attachments = await inbox.getDirectoryHandle("attachments", {
    create: true,
  });
  const key = "f".repeat(64);
  const end = vi.fn(async () => {});
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => dbPath },
    end,
  });
  mocks.create.mockResolvedValueOnce({
    storage: { path: async () => undefined },
    end: vi.fn(async () => {}),
  });
  const { result, unmount } = renderHook(useXMTP, { wrapper: XMTPProvider });
  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: mocks.signer as never,
    });
  });
  await attachments.getDirectoryHandle(key, { create: true });
  const getDirectory = vi
    .spyOn(navigator.storage, "getDirectory")
    .mockRejectedValueOnce(new Error("OPFS busy"));
  try {
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
    await expect(attachments.getDirectoryHandle(key)).rejects.toMatchObject({
      name: "NotFoundError",
    });
    expect(mocks.create).toHaveBeenCalledTimes(2);
  } finally {
    getDirectory.mockRestore();
    await backend.removeEntry(deployment, { recursive: true });
    await backend.removeEntry("deployments.json");
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
