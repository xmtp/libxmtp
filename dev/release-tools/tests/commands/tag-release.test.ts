import { describe, it, expect } from "vitest";
import { buildTag } from "../../src/commands/tag-release";

describe("buildTag", () => {
  it("builds iOS tag with prefix", () => {
    expect(buildTag("ios", "4.9.0")).toBe("ios-4.9.0");
  });

  it("builds Android tag with prefix", () => {
    expect(buildTag("android", "1.2.3")).toBe("android-1.2.3");
  });

  it("handles dev versions", () => {
    expect(buildTag("ios", "4.9.0-dev.abc1234")).toBe("ios-4.9.0-dev.abc1234");
    expect(buildTag("android", "1.2.3-dev.abc1234")).toBe(
      "android-1.2.3-dev.abc1234",
    );
  });

  it("handles rc versions", () => {
    expect(buildTag("ios", "4.9.0-rc1")).toBe("ios-4.9.0-rc1");
    expect(buildTag("android", "1.2.3-rc2")).toBe("android-1.2.3-rc2");
  });

  it("throws for unknown SDK", () => {
    expect(() => buildTag("unknown", "1.0.0")).toThrow("Unknown SDK: unknown");
  });

  it("throws for empty version", () => {
    expect(() => buildTag("ios", "")).toThrow("Invalid version");
  });

  it("throws for non-semver version", () => {
    expect(() => buildTag("ios", "not-a-version")).toThrow("Invalid version");
    expect(() => buildTag("android", "1.2")).toThrow("Invalid version");
  });
});
