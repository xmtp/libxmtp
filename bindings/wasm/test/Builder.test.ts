import { describe, expect, it } from "vitest";
import init, { BackendBuilder, WasmTestBuilder } from "../";

await init();

describe("WasmTestBuilder", () => {
  it("should set required fields and apply defaults", () => {
    const b = new WasmTestBuilder("hello");
    expect(b.name).toBe("hello");
    expect(b.flag).toBeUndefined();
    expect(b.count).toBeUndefined();
    expect(b.port).toBe(42);
    expect(b.enabled).toBe(true);
  });

  it("should support setter chaining", () => {
    const b = new WasmTestBuilder("chained")
      .setFlag(true)
      .setCount(99)
      .setPort(8080)
      .setEnabled(false);
    expect(b.name).toBe("chained");
    expect(b.flag).toBe(true);
    expect(b.count).toBe(99);
    expect(b.port).toBe(8080);
    expect(b.enabled).toBe(false);
  });

  it("should support partial chaining", () => {
    const b = new WasmTestBuilder("partial").setFlag(false);
    expect(b.name).toBe("partial");
    expect(b.flag).toBe(false);
    expect(b.count).toBeUndefined();
    expect(b.port).toBe(42);
    expect(b.enabled).toBe(true);
  });

  it("should allow defaults to be overridden", () => {
    const b = new WasmTestBuilder("defaults").setPort(9090).setEnabled(false);
    expect(b.port).toBe(9090);
    expect(b.enabled).toBe(false);
  });
});

describe("BackendBuilder", () => {
  it("API-client cache key uses backend URL and app version", () => {
    const url = "http://127.0.0.1:5050";
    const builder = new BackendBuilder(url);
    expect(builder.backendUrl).toBe(url);
    const first = builder.setEnv("local").setAppVersion("TestApp/1.0").build();
    const otherEnv = new BackendBuilder(url)
      .setEnv("custom")
      .setAppVersion("TestApp/1.0")
      .build();
    expect(first.cacheKey).toBe(`${url}|TestApp/1.0`);
    expect(otherEnv.cacheKey).toBe(first.cacheKey);
    expect(otherEnv.env).toBe("custom");
    const noVersion = new BackendBuilder(url).build();
    expect(noVersion.cacheKey).toBe(`${url}|`);
    expect(noVersion.cacheKey).not.toBe(first.cacheKey);
    const otherUrl = new BackendBuilder("http://127.0.0.1:59999").build();
    expect(otherUrl.cacheKey).not.toBe(noVersion.cacheKey);
  });

  it("backend URL is required", () => {
    // @ts-expect-error The backend URL is required.
    expect(() => new BackendBuilder()).toThrow();
    expect(() => new BackendBuilder("").build()).toThrow();
  });
});
