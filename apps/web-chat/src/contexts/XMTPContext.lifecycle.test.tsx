import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { XMTPProvider, useXMTP } from "./XMTPContext";

const mocks = vi.hoisted(() => {
  class StorageBusy extends Error {}
  return {
    StorageBusy,
    create: vi.fn(),
    releaseLock: vi.fn(),
    acquireLock: vi.fn(() => true),
    reset: vi.fn(),
    lockLost: null as null | (() => void),
    signer: {},
  };
});
vi.mock("@xmtp/browser-sdk", () => ({
  Client: { create: mocks.create },
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
