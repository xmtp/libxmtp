import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useMemberId } from "./useMemberId";

describe("useMemberId", () => {
  it("rejects invalid input", async () => {
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId("invalid"));
    expect(result.current.error).toBe("Invalid address or inbox ID");
  });

  it("accepts an inbox ID without a backend lookup", async () => {
    const inboxId = "a".repeat(64);
    const { result } = renderHook(() => useMemberId());
    await act(async () => result.current.setMemberId(inboxId));
    expect(result.current.inboxId).toBe(inboxId);
    expect(result.current.error).toBeNull();
  });
});
