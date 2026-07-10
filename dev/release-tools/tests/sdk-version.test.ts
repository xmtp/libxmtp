import { describe, it, expect } from "vitest";
import {
  diffBumpKind,
  applyBumpKind,
  resolveSdkVersion,
  capBumpKind,
} from "../src/lib/sdk-version";

describe("diffBumpKind", () => {
  it.each([
    ["1.10.0", "1.11.0", "minor"],
    ["1.10.0", "2.0.0", "major"],
    ["1.10.0", "1.10.1", "patch"],
  ])("diffBumpKind(%s, %s) => %s", (from, to, expected) => {
    expect(diffBumpKind(from, to)).toBe(expected);
  });

  it("throws when the target is not greater than the base", () => {
    expect(() => diffBumpKind("1.11.0", "1.10.0")).toThrow();
  });
});

describe("applyBumpKind", () => {
  it.each([
    ["4.10.0", "minor", "4.11.0"],
    ["4.10.0", "major", "5.0.0"],
    ["4.10.0", "patch", "4.10.1"],
  ])("applyBumpKind(%s, %s) => %s", (base, kind, expected) => {
    expect(applyBumpKind(base, kind as "minor")).toBe(expected);
  });
});

describe("resolveSdkVersion", () => {
  const pending = { version: "1.11.0", kind: "minor" as const };

  it("follows-libxmtp takes the pending number verbatim (final)", () => {
    expect(
      resolveSdkVersion({
        track: "follows-libxmtp",
        base: "1.10.0",
        pending,
        releaseType: "final",
      }),
    ).toBe("1.11.0");
  });

  it("independent mirrors the bump kind onto its own base (final)", () => {
    expect(
      resolveSdkVersion({
        track: "independent",
        base: "4.10.0",
        pending,
        releaseType: "final",
      }),
    ).toBe("4.11.0");
  });

  it("follows-libxmtp nightly previews the next number", () => {
    expect(
      resolveSdkVersion({
        track: "follows-libxmtp",
        base: "1.10.0",
        pending,
        releaseType: "nightly",
        nightlyDate: "20260603",
        shortSha: "abc1234",
      }),
    ).toBe("1.11.0-nightly.20260603.abc1234");
  });

  it("independent nightly previews the next number on its own base", () => {
    expect(
      resolveSdkVersion({
        track: "independent",
        base: "4.10.0",
        pending,
        releaseType: "nightly",
        nightlyDate: "20260603",
        shortSha: "abc1234",
      }),
    ).toBe("4.11.0-nightly.20260603.abc1234");
  });

  it("nightly requires date and sha", () => {
    expect(() =>
      resolveSdkVersion({
        track: "independent",
        base: "4.10.0",
        pending,
        releaseType: "nightly",
      }),
    ).toThrow();
  });
});

describe("capBumpKind", () => {
  it("clamps a major bump down to the max kind and recomputes the version", () => {
    expect(
      capBumpKind({ version: "2.0.0", kind: "major" }, "1.10.0", "minor"),
    ).toEqual({ version: "1.11.0", kind: "minor" });
  });

  it("leaves a bump at or below the cap unchanged", () => {
    expect(
      capBumpKind({ version: "1.11.0", kind: "minor" }, "1.10.0", "minor"),
    ).toEqual({ version: "1.11.0", kind: "minor" });
    expect(
      capBumpKind({ version: "1.10.1", kind: "patch" }, "1.10.0", "minor"),
    ).toEqual({ version: "1.10.1", kind: "patch" });
  });

  it("a cap above the computed kind is a no-op (never raises a bump)", () => {
    expect(
      capBumpKind({ version: "1.10.1", kind: "patch" }, "1.10.0", "major"),
    ).toEqual({ version: "1.10.1", kind: "patch" });
  });

  it("clamps major down to patch when capped at patch", () => {
    expect(
      capBumpKind({ version: "2.0.0", kind: "major" }, "1.10.0", "patch"),
    ).toEqual({ version: "1.10.1", kind: "patch" });
  });
});
