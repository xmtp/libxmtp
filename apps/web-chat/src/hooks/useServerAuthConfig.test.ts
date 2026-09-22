import { act, renderHook, waitFor } from "@testing-library/react";
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

  it("clears the previous backend result immediately for an invalid URL", async () => {
    mockConfiguration({ enabled: false });
    const { result, rerender } = renderHook(
      ({ url }) => useServerAuthConfig(url),
      { initialProps: { url: "https://backend.example.com" } },
    );
    await waitFor(() => expect(result.current.required).toBe(false));

    rerender({ url: "invalid" });
    expect(result.current).toEqual({
      required: true,
      requiredScopes: [],
      loading: false,
    });
  });

  it("ignores a previous URL response while the current backend is pending", async () => {
    type Configuration = Awaited<
      ReturnType<typeof Client.fetchServerConfiguration>
    >;
    const first = Promise.withResolvers<Configuration>();
    const second = Promise.withResolvers<Configuration>();
    vi.spyOn(Client, "fetchServerConfiguration")
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(
      ({ url }) => useServerAuthConfig(url),
      { initialProps: { url: "https://first.example.com" } },
    );
    rerender({ url: "https://second.example.com" });
    await act(async () => {
      first.resolve({
        auth: { enabled: false, requiredScopes: [] },
      } as unknown as Configuration);
      await first.promise;
    });
    expect(result.current).toEqual({
      required: true,
      requiredScopes: [],
      loading: true,
    });
    await act(async () => {
      second.resolve({
        auth: { enabled: true, requiredScopes: ["read"] },
      } as unknown as Configuration);
      await second.promise;
    });
    expect(result.current.requiredScopes).toEqual(["read"]);
    expect(result.current.loading).toBe(false);
  });

  it("requires a fresh response when returning to an earlier URL", async () => {
    const fetch = mockConfiguration({ enabled: false });
    const { result, rerender } = renderHook(
      ({ url }) => useServerAuthConfig(url),
      { initialProps: { url: "https://backend.example.com" } },
    );
    await waitFor(() => expect(result.current.required).toBe(false));
    rerender({ url: "" });
    const pending =
      Promise.withResolvers<
        Awaited<ReturnType<typeof Client.fetchServerConfiguration>>
      >();
    fetch.mockReturnValueOnce(pending.promise);
    rerender({ url: "https://backend.example.com" });
    expect(result.current).toEqual({
      required: true,
      requiredScopes: [],
      loading: true,
    });
  });
});
