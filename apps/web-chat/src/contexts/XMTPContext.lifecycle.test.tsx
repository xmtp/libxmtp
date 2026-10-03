import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

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
    generateInboxId: vi.fn().mockReturnValue("legacy-nonce-one-id"),
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
vi.mock("@xmtp/browser-sdk", () => ({
  Client: {
    create: mocks.create,
    canMessage: mocks.canMessage,
    inboxIdFor: mocks.inboxIdFor,
  },
  generateInboxId: mocks.generateInboxId,
  Storage: {
    admin: () =>
      Promise.resolve({ listFiles: mocks.listFiles, end: mocks.endAdmin }),
  },
  initLogging: vi.fn(),
  XmtpError: { StorageBusy: mocks.StorageBusy },
}));
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
  mocks.create.mockResolvedValueOnce({ end: vi.fn() });
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
  mocks.create.mockResolvedValueOnce({ end: vi.fn() });
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
  const identity = { identifier: "0x1234", kind: "ethereum" };
  const signer = { identity: vi.fn().mockResolvedValue(identity) };
  mocks.listFiles.mockResolvedValueOnce(["xmtp-test-legacy-nonce-one-id.db3"]);
  mocks.canMessage.mockResolvedValueOnce(
    new Map([[`${identity.kind}:${identity.identifier}`, false]]),
  );
  mocks.create.mockResolvedValueOnce({ end: vi.fn() });
  const { result } = renderHook(useXMTP, { wrapper: XMTPProvider });

  await act(async () => {
    await result.current.initialize({
      backendUrl: "https://example.com",
      env: "test",
      signer: signer as never,
    });
  });

  expect(mocks.generateInboxId).toHaveBeenCalledWith(identity);
  expect(mocks.inboxIdFor).not.toHaveBeenCalled();
  expect(mocks.create).toHaveBeenCalledWith(
    signer,
    expect.objectContaining({
      storage: {
        location: {
          dbPath: "xmtp-test-legacy-nonce-one-id.db3",
          attachmentsDir: "xmtp-test-legacy-nonce-one-id.db3.attachments",
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
  mocks.create.mockResolvedValueOnce({ end: () => held.promise });
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

it("disconnects and reports a failed SDK end after lock loss", async () => {
  const end = vi.fn().mockRejectedValueOnce(new Error("Shutdown failed"));
  mocks.create.mockResolvedValueOnce({ end });
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

it("ends a client created after another tab takes the app lock", async () => {
  const created = Promise.withResolvers<{ end: () => Promise<void> }>();
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
    created.resolve({ end });
    await expect(pending).rejects.toThrow("App lock was lost");
  });

  expect(end).toHaveBeenCalledTimes(1);
  expect(result.current.client).toBeUndefined();
  expect(result.current.signer).toBeUndefined();
});

it("checks storage ownership when the lock-loss event is delayed", async () => {
  const created = Promise.withResolvers<{ end: () => Promise<void> }>();
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
    created.resolve({ end });
    await expect(pending).rejects.toThrow("App lock was lost");
  });

  expect(end).toHaveBeenCalledTimes(1);
  expect(result.current.client).toBeUndefined();
});
