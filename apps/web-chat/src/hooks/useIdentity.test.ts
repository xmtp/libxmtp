import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useIdentity } from "./useIdentity";

const mocks = vi.hoisted(() => ({
  client: {
    preferences: { fetchInboxState: vi.fn() },
    fetchKeyPackageStatuses: vi.fn(async () => new Map()),
  },
}));
vi.mock("@/contexts/XMTPContext", () => ({ useClient: () => mocks.client }));

const state = (inboxId: string) => ({
  inboxId,
  installations: [],
  accountIdentifiers: [],
  recoveryIdentifier: null,
});

beforeEach(() => vi.clearAllMocks());

describe("useIdentity", () => {
  it("derives initial pending state and does not refetch on render", async () => {
    const pending = Promise.withResolvers<ReturnType<typeof state>>();
    mocks.client.preferences.fetchInboxState.mockReturnValueOnce(
      pending.promise,
    );
    const { result, rerender, unmount } = renderHook(() => useIdentity(true));
    expect(result.current.syncing).toBe(true);
    rerender();
    expect(mocks.client.preferences.fetchInboxState).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve(state("first")));
    await waitFor(() => expect(result.current.syncing).toBe(false));
    expect(result.current.inboxId).toBe("first");
    unmount();
  });

  it("supports manual refresh without an initial fetch", async () => {
    const pending = Promise.withResolvers<ReturnType<typeof state>>();
    mocks.client.preferences.fetchInboxState.mockReturnValueOnce(
      pending.promise,
    );
    const { result, unmount } = renderHook(() => useIdentity());
    expect(result.current.syncing).toBe(false);
    expect(mocks.client.preferences.fetchInboxState).not.toHaveBeenCalled();
    let refresh: Promise<void>;
    act(() => {
      refresh = result.current.sync();
    });
    expect(result.current.syncing).toBe(true);
    await act(async () => {
      pending.resolve(state("manual"));
      await refresh;
    });
    expect(result.current.inboxId).toBe("manual");
    expect(result.current.syncing).toBe(false);
    unmount();
  });

  it("keeps the latest refresh when an older initial fetch finishes late", async () => {
    const pending = Promise.withResolvers<ReturnType<typeof state>>();
    mocks.client.preferences.fetchInboxState
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(state("new"));
    const { result, unmount } = renderHook(() => useIdentity(true));
    await act(async () => result.current.sync());
    await act(async () => pending.resolve(state("old")));
    expect(result.current.inboxId).toBe("new");
    expect(result.current.syncing).toBe(false);
    unmount();
  });
});
