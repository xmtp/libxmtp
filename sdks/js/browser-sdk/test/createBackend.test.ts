import { describe, expect, it } from "vitest";
import { createBackend } from "@/utils/createBackend";

describe("createBackend", () => {
  it("should create a backend with appVersion", async () => {
    const backend = await createBackend({
      backendUrl: "https://backend.example.com",
      appVersion: "test/1.0.0",
    });
    expect(backend).toBeDefined();
    expect(backend.appVersion).toBe("test/1.0.0");
  });

  it("should create a backend with an explicit URL", async () => {
    const backend = await createBackend({
      backendUrl: "https://custom-api.example.com",
    });
    expect(backend).toBeDefined();
    expect(backend.backendUrl).toBe("https://custom-api.example.com");
    expect(backend.env).toBeUndefined();
  });

  it("should key API clients by backend URL and app version only", async () => {
    const options = {
      backendUrl: "https://backend.example.com",
      appVersion: "test/1.0.0",
    };
    const first = await createBackend({ ...options, env: "first" });
    const second = await createBackend({ ...options, env: "second" });
    const otherUrl = await createBackend({
      ...options,
      backendUrl: "https://other.example.com",
    });
    const otherVersion = await createBackend({
      ...options,
      appVersion: "test/2.0.0",
    });
    expect(first.cacheKey).toBe(`${options.backendUrl}|${options.appVersion}`);
    expect(second.cacheKey).toBe(first.cacheKey);
    expect(otherUrl.cacheKey).not.toBe(first.cacheKey);
    expect(otherVersion.cacheKey).not.toBe(first.cacheKey);
    expect(
      (await createBackend({ backendUrl: options.backendUrl })).cacheKey,
    ).toBe(`${options.backendUrl}|`);
  });
});
