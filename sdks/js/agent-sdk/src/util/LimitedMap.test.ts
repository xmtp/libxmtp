import { describe, it, expect } from "vitest";
import { LimitedMap } from "./LimitedMap.js";

describe("LimitedMap", () => {
  it("stores and retrieves values", () => {
    const map = new LimitedMap<string, number>(3);
    map.set("a", 1);
    map.set("b", 2);

    expect(map.get("a")).toBe(1);
    expect(map.get("b")).toBe(2);
    expect(map.get("c")).toBeUndefined();
  });

  it("evicts oldest entry when inserting a new key at capacity", () => {
    const map = new LimitedMap<string, number>(2);
    map.set("a", 1);
    map.set("b", 2);
    map.set("c", 3);

    expect(map.get("a")).toBeUndefined();
    expect(map.get("b")).toBe(2);
    expect(map.get("c")).toBe(3);
  });

  it("does not evict entries when updating an existing key at capacity", () => {
    const map = new LimitedMap<string, number>(2);
    map.set("a", 1);
    map.set("b", 2);

    // Update existing key "b"
    map.set("b", 4);

    expect(map.get("a")).toBe(1);
    expect(map.get("b")).toBe(4);
  });

  it("maintains correct capacity after updates and subsequent new insertions", () => {
    const map = new LimitedMap<string, number>(2);
    map.set("a", 1);
    map.set("b", 2);

    // Update "a" and "b"
    map.set("a", 10);
    map.set("b", 20);

    expect(map.get("a")).toBe(10);
    expect(map.get("b")).toBe(20);

    // Insert new key "c", should evict "a" (oldest insertion)
    map.set("c", 30);
    expect(map.get("a")).toBeUndefined();
    expect(map.get("b")).toBe(20);
    expect(map.get("c")).toBe(30);
  });
});
