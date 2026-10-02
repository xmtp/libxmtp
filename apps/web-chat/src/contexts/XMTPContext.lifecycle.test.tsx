import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { XMTPProvider, useXMTP } from "./XMTPContext";

const mocks = vi.hoisted(() => {
  class StorageBusy extends Error {}
  return {
    StorageBusy,
    create: vi.fn(),
    releaseLock: vi.fn(),
    acquireLock: vi.fn(() => true),
    signer: {},
  };
});
vi.mock("@xmtp/browser-sdk", () => ({
  Client: { create: mocks.create },
  initLogging: vi.fn(),
  XmtpError: { StorageBusy: mocks.StorageBusy },
}));
vi.mock("@/hooks/useAppLock", () => ({
  useAppLock: () => ({
    lockState: "available",
    acquireLock: mocks.acquireLock,
    releaseLock: mocks.releaseLock,
  }),
}));
vi.mock("@/stores/inbox/hooks", () => ({
  useActions: () => ({ reset: vi.fn() }),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
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
