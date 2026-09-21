import { describe, expect, it } from "vitest";
import { readCredential } from "@/utils/auth";

describe("credential validation", () => {
  it.each([NaN, Infinity, 0.5, Number.MAX_SAFE_INTEGER + 1])(
    "rejects unsafe expiration %s before passing it to native bindings",
    async (expiresAtSeconds) => {
      await expect(
        readCredential(async () => ({
          value: "Bearer secret",
          expiresAtSeconds,
        })),
      ).rejects.toThrow(/^auth callback failed$/);
    },
  );

  it("does not retain the callback error or its cause", async () => {
    const failure = new Error("private refresh response");
    const error: unknown = await readCredential(() =>
      Promise.reject(failure),
    ).catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(Error);
    expect(error).toHaveProperty("message", "auth callback failed");
    expect(error).not.toHaveProperty("cause");
  });
});
