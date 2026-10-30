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

const A = "https://a.example";
const B = "https://b.example";

const seed = (token: string, url = A) => {
  localStorage.setItem("XMTP_AUTH_TOKENS", JSON.stringify({ [url]: token }));
};

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
    localStorage.setItem("XMTP_BACKEND_URL", JSON.stringify(A));
  });

  it("probes with an empty credential so an open backend connects untouched", async () => {
    const { result } = renderAuthToken();
    const callback = result.current.createCredentialSource(A);

    // The middleware asks before the first request whether or not the
    // deployment requires auth, so the first ask must not interrupt the user.
    await expect(callback.credential()).resolves.toMatchObject({ value: "" });
    expect(result.current.request).toBeNull();
  });

  it("keeps one identity for an open prompt and changes it for a new prompt", () => {
    const { result } = renderAuthToken();
    act(() => {
      result.current.promptForToken();
    });
    const firstId = result.current.request?.id;
    expect(firstId).toBeDefined();
    act(() => {
      result.current.promptForToken();
    });
    expect(result.current.request?.id).toBe(firstId);
    act(() => {
      result.current.request?.resolve("token");
      result.current.promptForToken();
    });
    expect(result.current.request?.id).not.toBe(firstId);
  });

  it("reports a rejection when the same consumer is asked again", async () => {
    seed("stale-token");
    const { result } = renderAuthToken();
    const callback = result.current.createCredentialSource(A);

    await expect(callback.credential()).resolves.toMatchObject({
      value: "Bearer stale-token",
    });

    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = callback.credential();
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
    seed("good-token");
    const { result } = renderAuthToken();

    // A reconnect, or an inbox tools query, builds a separate client with its
    // own credential cache. Its first ask must be answered from storage.
    const first = result.current.createCredentialSource(A);
    await expect(first.credential()).resolves.toMatchObject({
      value: "Bearer good-token",
    });

    const second = result.current.createCredentialSource(A);
    await expect(second.credential()).resolves.toMatchObject({
      value: "Bearer good-token",
    });
    expect(result.current.request).toBeNull();
  });

  it("resolves every concurrent waiter from one submission", async () => {
    seed("stale-token");
    const { result } = renderAuthToken();
    const first = result.current.createCredentialSource(A);
    const second = result.current.createCredentialSource(A);

    // Both consumers offer the stale token and are refused.
    await first.credential();
    await second.credential();

    let a: Promise<{ value: string }> | undefined;
    let b: Promise<{ value: string }> | undefined;
    act(() => {
      a = first.credential();
      b = second.credential();
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
    seed("stale-token");
    const { result } = renderAuthToken();
    const callback = result.current.createCredentialSource(A);
    await callback.credential();

    act(() => {
      void callback.credential();
    });
    act(() => {
      result.current.request?.resolve("fresh-token");
    });

    // The consumer already refused "stale-token"; the token entered since is
    // newer, so it is offered rather than read as already refused.
    await expect(callback.credential()).resolves.toMatchObject({
      value: "Bearer fresh-token",
    });
  });

  it.each(["", "stale-token"])(
    "offers the submitted token before React commits (stored token: %j)",
    async (storedToken) => {
      seed(storedToken);
      const { result } = renderAuthToken();
      const callback = result.current.createCredentialSource(A);
      await callback.credential();

      let waiting: Promise<{ value: string }> | undefined;
      act(() => {
        waiting = callback.credential();
      });

      let immediate: Promise<{ value: string }> | undefined;
      act(() => {
        result.current.request?.resolve("  fresh-token  ");
        immediate = callback.credential();
      });

      await expect(waiting).resolves.toMatchObject({
        value: "Bearer fresh-token",
      });
      await expect(immediate).resolves.toMatchObject({
        value: "Bearer fresh-token",
      });
      expect(result.current.request).toBeNull();
    },
  );

  it("resolves both concurrent calls that share one consumer's callback", async () => {
    // The InboxTools case: "Find installations" and "Check updates count"
    // started together, with no stored token, both using that panel's single
    // callback. Neither may be left pending.
    const { result } = renderAuthToken();
    const callback = result.current.createCredentialSource(A);

    // The empty probe is refused, so the next asks must prompt.
    await callback.credential();

    let a: Promise<{ value: string }> | undefined;
    let b: Promise<{ value: string }> | undefined;
    act(() => {
      a = callback.credential();
      b = callback.credential();
    });
    expect(result.current.request).not.toBeNull();
    // Nothing was supplied yet, so this is a first ask, not a rejection.
    expect(result.current.request?.rejected).toBe(false);

    act(() => {
      result.current.request?.resolve("one-token");
    });
    await expect(a).resolves.toMatchObject({ value: "Bearer one-token" });
    await expect(b).resolves.toMatchObject({ value: "Bearer one-token" });
    expect(result.current.request).toBeNull();
  });

  it("keeps each backend's token for that backend", async () => {
    seed("tA");
    const { result } = renderAuthToken();
    // A source for another backend never reads the current backend's token.
    const toB = result.current.createCredentialSource(B);
    await expect(toB.credential()).resolves.toMatchObject({ value: "" });

    let credential: Promise<{ value: string }> | undefined;
    act(() => {
      credential = toB.credential();
    });
    expect(result.current.request?.origin).toBe(B);

    act(() => {
      result.current.request?.resolve("tB");
    });
    await expect(credential).resolves.toMatchObject({ value: "Bearer tB" });
    await expect(
      result.current.createCredentialSource(A).credential(),
    ).resolves.toMatchObject({ value: "Bearer tA" });
    await expect(
      result.current.createCredentialSource(B).credential(),
    ).resolves.toMatchObject({ value: "Bearer tB" });
  });

  it("gives a token only to waiters for the backend it was entered for", async () => {
    const { result } = renderAuthToken();
    const toA = result.current.createCredentialSource(A);
    const toB = result.current.createCredentialSource(B);
    await toA.credential();
    await toB.credential();

    let a: Promise<{ value: string }> | undefined;
    let b: Promise<{ value: string }> | undefined;
    act(() => {
      a = toA.credential();
      b = toB.credential();
    });
    expect(result.current.request?.origin).toBe(A);

    act(() => {
      result.current.request?.resolve("tA");
    });
    await expect(a).resolves.toMatchObject({ value: "Bearer tA" });
    // The other waiter is released empty and asks again for its own backend.
    await expect(b).resolves.toMatchObject({ value: "" });
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

    const callback = result.current.createCredentialSource(A);
    await expect(callback.credential()).resolves.toMatchObject({
      value: "Bearer early-token",
    });
  });
});
