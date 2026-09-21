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

  it("probes with an empty credential so an open backend connects untouched", async () => {
    const { result } = renderAuthToken();
    const callback = result.current.createAuthCallback();

    // The middleware asks before the first request whether or not the
    // deployment requires auth, so the first ask must not interrupt the user.
    await expect(callback()).resolves.toMatchObject({ value: "" });
    expect(result.current.request).toBeNull();
  });

  it("prompts once the empty probe is refused", async () => {
    const { result } = renderAuthToken();
    const callback = result.current.createAuthCallback();
    await callback();

    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = callback();
    });
    expect(result.current.request).not.toBeNull();
    // Nothing was supplied yet, so this is a first ask, not a rejection.
    expect(result.current.request?.rejected).toBe(false);

    act(() => {
      result.current.request?.resolve("abc123");
    });
    await expect(credential).resolves.toMatchObject({ value: "Bearer abc123" });
    expect(result.current.request).toBeNull();
  });

  it("offers a stored token before prompting", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stored-token"));
    const { result } = renderAuthToken();
    const callback = result.current.createAuthCallback();

    await expect(callback()).resolves.toMatchObject({
      value: "Bearer stored-token",
    });
    expect(result.current.request).toBeNull();
  });

  it("reports a rejection when the same consumer is asked again", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stale-token"));
    const { result } = renderAuthToken();
    const callback = result.current.createAuthCallback();

    await expect(callback()).resolves.toMatchObject({
      value: "Bearer stale-token",
    });

    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = callback();
    });
    expect(result.current.request?.rejected).toBe(true);

    act(() => {
      result.current.request?.resolve("fresh-token");
    });
    await expect(credential).resolves.toMatchObject({
      value: "Bearer fresh-token",
    });
  });

  it("does not treat a new consumer's first ask as a rejection", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("good-token"));
    const { result } = renderAuthToken();

    // A reconnect, or an inbox tools query, builds a separate client with its
    // own credential cache. Its first ask must be answered from storage.
    const first = result.current.createAuthCallback();
    await expect(first()).resolves.toMatchObject({
      value: "Bearer good-token",
    });

    const second = result.current.createAuthCallback();
    await expect(second()).resolves.toMatchObject({
      value: "Bearer good-token",
    });
    expect(result.current.request).toBeNull();
  });

  it("resolves every concurrent waiter from one submission", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stale-token"));
    const { result } = renderAuthToken();
    const first = result.current.createAuthCallback();
    const second = result.current.createAuthCallback();

    // Both consumers offer the stale token and are refused.
    await first();
    await second();

    let a: Promise<{ value: string }> | undefined;
    let b: Promise<{ value: string }> | undefined;
    act(() => {
      a = first();
      b = second();
    });
    expect(result.current.request).not.toBeNull();

    act(() => {
      result.current.request?.resolve("fresh-token");
    });

    // Neither call may be left pending: an unresolved callback holds the
    // backend's refresh lock and never releases its client resources.
    await expect(a).resolves.toMatchObject({ value: "Bearer fresh-token" });
    await expect(b).resolves.toMatchObject({ value: "Bearer fresh-token" });
  });

  it("offers a newly entered token to a consumer that was already refused", async () => {
    localStorage.setItem("XMTP_AUTH_TOKEN", JSON.stringify("stale-token"));
    const { result } = renderAuthToken();
    const callback = result.current.createAuthCallback();
    await callback();

    act(() => {
      void callback();
    });
    act(() => {
      result.current.request?.resolve("fresh-token");
    });

    // The consumer already refused "stale-token"; the token entered since is
    // newer, so it is offered rather than read as already refused.
    await expect(callback()).resolves.toMatchObject({
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

    const callback = result.current.createAuthCallback();
    await expect(callback()).resolves.toMatchObject({
      value: "Bearer early-token",
    });
  });
});
