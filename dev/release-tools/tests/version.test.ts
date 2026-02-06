import { describe, it, expect } from "vitest";
import {
  computeVersion,
  filterAndSortTags,
  normalizeVersion,
} from "../src/lib/version";

describe("filterAndSortTags", () => {
  it("filters tags by prefix and sorts descending", () => {
    const tags = [
      "ios-4.8.0",
      "ios-4.9.0",
      "android-5.0.0",
      "ios-4.10.0",
      "ios-4.9.0-libxmtp",
    ];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual(["4.10.0", "4.9.0", "4.8.0"]);
  });

  it("includes prerelease tags when flag is set", () => {
    const tags = ["ios-4.9.0", "ios-4.10.0-rc1", "ios-4.10.0-dev.abc1234"];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp", true);
    expect(result).toEqual(["4.10.0-rc1", "4.10.0-dev.abc1234", "4.9.0"]);
  });

  it("excludes prerelease tags by default", () => {
    const tags = ["ios-4.9.0", "ios-4.10.0-rc1", "ios-4.10.0-dev.abc1234"];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual(["4.9.0"]);
  });

  it("returns empty array when no tags match", () => {
    const tags = ["android-5.0.0", "kotlin-bindings-1.0.0"];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp");
    expect(result).toEqual([]);
  });

  it("excludes artifact tags ending in suffix", () => {
    const tags = ["ios-4.9.0", "ios-4.9.0-libxmtp", "ios-4.10.0-libxmtp"];
    const result = filterAndSortTags(tags, "ios-", "-libxmtp", true);
    expect(result).toEqual(["4.9.0"]);
  });
});

describe("normalizeVersion", () => {
  it.each([
    ["4.9.0", "4.9.0"],
    ["4.9.0-dev.abc1234", "4.9.0"],
    ["4.9.0-rc1", "4.9.0"],
    ["4.9.0+build.123", "4.9.0"],
    ["4.9.0-rc1+build.123", "4.9.0"],
  ])("normalizeVersion(%s) => %s", (input, expected) => {
    expect(normalizeVersion(input)).toBe(expected);
  });

  it.each(["invalid", ""])("throws on invalid input: %s", (input) => {
    expect(() => normalizeVersion(input)).toThrow("Invalid version format");
  });
});

describe("computeVersion", () => {
  it("returns base version for final release", () => {
    expect(computeVersion("4.10.0", "final")).toBe("4.10.0");
  });

  it("appends rc suffix for rc release", () => {
    expect(computeVersion("4.10.0", "rc", { rcNumber: 1 })).toBe("4.10.0-rc1");
  });

  it("appends dev suffix with short sha", () => {
    expect(computeVersion("4.10.0", "dev", { shortSha: "abc1234" })).toBe(
      "4.10.0-dev.abc1234",
    );
  });

  it("throws if rc release has no rcNumber", () => {
    expect(() => computeVersion("4.10.0", "rc")).toThrow();
  });

  it("throws if dev release has no shortSha", () => {
    expect(() => computeVersion("4.10.0", "dev")).toThrow();
  });
});
