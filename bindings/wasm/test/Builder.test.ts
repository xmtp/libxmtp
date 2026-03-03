import { describe, expect, it } from "vitest";
import init, { WasmTestBuilder } from "../";

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
