import { renderHook, waitFor } from "@testing-library/react";
import { Client } from "@xmtp/browser-sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useServerAuthConfig } from "./useServerAuthConfig";

const mockConfiguration = (auth: {
  enabled: boolean;
  requiredScopes?: string[];
}) =>
  vi.spyOn(Client, "fetchServerConfiguration").mockResolvedValue({
    auth: { requiredScopes: [], ...auth },
  } as unknown as Awaited<ReturnType<typeof Client.fetchServerConfiguration>>);

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useServerAuthConfig", () => {
  it("reports no token needed when the backend says auth is disabled", async () => {
    mockConfiguration({ enabled: false });
    const { result } = renderHook(() =>
      useServerAuthConfig("https://backend.example.com"),
    );
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
    expect(result.current.required).toBe(false);
  });

  it("surfaces the scopes the backend asks for", async () => {
    mockConfiguration({ enabled: true, requiredScopes: ["read", "write"] });
    const { result } = renderHook(() =>
      useServerAuthConfig("https://backend.example.com"),
    );
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
    expect(result.current.required).toBe(true);
    expect(result.current.requiredScopes).toEqual(["read", "write"]);
  });

  it("keeps the token field available when the backend cannot be read", async () => {
    // Hiding the field on a failed read would strand a user on a backend that
    // does require a token.
    vi.spyOn(Client, "fetchServerConfiguration").mockRejectedValue(
      new Error("unreachable"),
    );
    const { result } = renderHook(() =>
      useServerAuthConfig("https://backend.example.com"),
    );
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
    expect(result.current.required).toBe(true);
  });

  it("does not query an invalid URL", () => {
    const fetchSpy = mockConfiguration({ enabled: false });
    const { result } = renderHook(() => useServerAuthConfig(""));
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(result.current.required).toBe(true);
    expect(result.current.loading).toBe(false);
  });
});
