import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useMemberId } from "./useMemberId";

const mocks = vi.hoisted(() => ({
  getInboxIdForAddressQuery: vi.fn(),
  getInboxIdForHandleQuery: vi.fn(),
}));
vi.mock("@/helpers/inboxId", () => mocks);

beforeEach(() => vi.clearAllMocks());

describe("useMemberId", () => {
  it("rejects invalid input", async () => {
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId("nick.eth"));
    expect(result.current.error).toBe("Invalid address, inbox ID, or handle");
  });

  it("accepts an inbox ID without a backend lookup", async () => {
    const inboxId = "a".repeat(64);
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId(inboxId));
    expect(result.current.inboxId).toBe(inboxId);
    expect(result.current.error).toBeNull();
    expect(mocks.getInboxIdForHandleQuery).not.toHaveBeenCalled();
  });

  it("resolves a handle and shows the inbox's current handle", async () => {
    const inboxId = "b".repeat(64);
    mocks.getInboxIdForHandleQuery.mockResolvedValue({
      inboxId,
      handle: "nick#2341",
    });
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId("nick"));
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.inboxId).toBe(inboxId));
    expect(result.current.handle).toBe("nick#2341");
    expect(result.current.error).toBeNull();
    expect(mocks.getInboxIdForHandleQuery).toHaveBeenCalledWith(
      "nick",
      expect.any(String),
    );
  });

  it("reports a handle that no inbox holds", async () => {
    mocks.getInboxIdForHandleQuery.mockResolvedValue(null);
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId("nobody"));
    await waitFor(() =>
      expect(result.current.error).toBe(
        "No inbox has this handle on this server",
      ),
    );
    expect(result.current.inboxId).toBe("");
  });
});
