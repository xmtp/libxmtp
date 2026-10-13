import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useMemberId } from "./useMemberId";

const mocks = vi.hoisted(() => {
  const source = {
    credential: async () => ({ value: "", expiresAtSeconds: 0n }),
  };
  return {
    getInboxIdForAddressQuery: vi.fn(),
    getInboxIdForHandleQuery: vi.fn(),
    source,
    createCredentialSource: vi.fn(() => source),
  };
});
vi.mock("@/helpers/inboxId", () => ({
  getInboxIdForAddressQuery: mocks.getInboxIdForAddressQuery,
  getInboxIdForHandleQuery: mocks.getInboxIdForHandleQuery,
}));
vi.mock("@/contexts/AuthTokenContext", () => ({
  useAuthToken: () => ({
    createCredentialSource: mocks.createCredentialSource,
  }),
}));

// a lookup connects its own backend client, with the app's credential source
const credentialed = { url: expect.any(String), credentials: mocks.source };

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
      credentialed,
    );
    expect(mocks.createCredentialSource).toHaveBeenCalledTimes(1);
  });

  it("looks up an address with the app's credential source", async () => {
    const address = `0x${"A".repeat(40)}`;
    mocks.getInboxIdForAddressQuery.mockResolvedValue("c".repeat(64));
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId(address));
    await waitFor(() => expect(result.current.inboxId).toBe("c".repeat(64)));
    expect(mocks.getInboxIdForAddressQuery).toHaveBeenCalledWith(
      address.toLowerCase(),
      credentialed,
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
