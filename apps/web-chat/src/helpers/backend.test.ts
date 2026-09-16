import { describe, expect, it } from "vitest";
import { backendHost, backendLabel, isValidBackendUrl } from "./backend";

describe("backend helpers", () => {
  it("validates only HTTP backends", () => {
    expect(isValidBackendUrl("http://127.0.0.1:5050")).toBe(true);
    expect(isValidBackendUrl("https://example.com/path")).toBe(true);
    expect(isValidBackendUrl("wss://example.com")).toBe(false);
    expect(isValidBackendUrl("not a URL")).toBe(false);
  });

  it("returns the backend host", () => {
    expect(backendHost("http://127.0.0.1:5050/path")).toBe("127.0.0.1:5050");
  });

  it("matches the CLI label rules", async () => {
    await expect(backendLabel("http://localhost:5050")).resolves.toBe(
      "localhost-5050-50a235",
    );
    await expect(backendLabel("https://example.com/path")).resolves.toBe(
      "example-com-443-100680",
    );
  });

  it("uses the origin hash to avoid sanitized host collisions", async () => {
    await expect(backendLabel("http://a_b:5050")).resolves.toBe(
      "a-b-5050-adcf36",
    );
    await expect(backendLabel("http://a-b:5050")).resolves.toBe(
      "a-b-5050-026174",
    );
  });
});
