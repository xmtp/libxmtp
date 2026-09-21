import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import {
  AuthTokenProvider,
  useAuthToken,
  withBearerPrefix,
} from "./AuthTokenContext";

const wrapper = ({ children }: React.PropsWithChildren) => (
  <AuthTokenProvider>{children}</AuthTokenProvider>
);

const renderAuthToken = () => renderHook(() => useAuthToken(), { wrapper });

describe("withBearerPrefix", () => {
  it("adds Bearer to a bare token", () => {
    expect(withBearerPrefix("abc123")).toBe("Bearer abc123");
    expect(withBearerPrefix("  abc123  ")).toBe("Bearer abc123");
  });

  it("leaves an existing scheme alone", () => {
    expect(withBearerPrefix("Bearer abc123")).toBe("Bearer abc123");
    expect(withBearerPrefix("Token abc123")).toBe("Token abc123");
  });
});

describe("AuthTokenProvider", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("prompts when no token is stored and resolves what the user submits", async () => {
    const { result } = renderAuthToken();

    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = result.current.authCallback();
    });
    // The promise stays pending so the backend waits rather than failing.
    expect(result.current.request).not.toBeNull();
    expect(result.current.request?.rejected).toBe(false);

    act(() => {
      result.current.request?.resolve("abc123");
    });
    await expect(credential).resolves.toMatchObject({
      value: "Bearer abc123",
    });
    expect(result.current.request).toBeNull();
  });

  it("resolves from storage without prompting", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stored-token"));
    const { result } = renderAuthToken();

    const credential = result.current.authCallback();
    await expect(credential).resolves.toMatchObject({
      value: "Bearer stored-token",
    });
    expect(result.current.request).toBeNull();
  });

  it("prompts as rejected when the backend asks again for the same token", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stale-token"));
    const { result } = renderAuthToken();

    // First ask is satisfied from storage.
    await expect(result.current.authCallback()).resolves.toMatchObject({
      value: "Bearer stale-token",
    });

    // A second ask for the same value means the backend rejected it, so the
    // user is prompted rather than handed the known-bad token again.
    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = result.current.authCallback();
    });
    expect(result.current.request?.rejected).toBe(true);

    act(() => {
      result.current.request?.resolve("fresh-token");
    });
    await expect(credential).resolves.toMatchObject({
      value: "Bearer fresh-token",
    });
  });

  it("accepts a token entered before any backend request", async () => {
    const { result } = renderAuthToken();

    act(() => {
      result.current.promptForToken();
    });
    expect(result.current.request).not.toBeNull();

    act(() => {
      result.current.request?.resolve("early-token");
    });
    expect(result.current.request).toBeNull();

    // The token entered up front satisfies the first backend request.
    await expect(result.current.authCallback()).resolves.toMatchObject({
      value: "Bearer early-token",
    });
  });
});
