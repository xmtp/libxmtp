import { describe, it, expect, vi } from "vitest";
import semver from "semver";
import {
  computeVersion,
  filterAndSortTags,
  getTimestamp,
  normalizeVersion,
  validateTimestamp,
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
    ["4.9.0-nightly.20260428.cc66682", "4.9.0"],
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

  it("appends nightly suffix with timestamp and short sha", () => {
    expect(
      computeVersion("4.10.0", "nightly", {
        timestamp: "20260428060000",
        shortSha: "cc66682",
      }),
    ).toBe("4.10.0-pre.20260428060000.nightly.cc66682");
  });

  it("throws if nightly release has no timestamp", () => {
    expect(() =>
      computeVersion("4.10.0", "nightly", { shortSha: "cc66682" }),
    ).toThrow(/timestamp/);
  });

  it("throws if nightly release has no shortSha", () => {
    expect(() =>
      computeVersion("4.10.0", "nightly", { timestamp: "20260428060000" }),
    ).toThrow(/shortSha/);
  });

  it("nightly versions sort lexicographically by timestamp", () => {
    const versions = [
      computeVersion("4.10.0", "nightly", {
        timestamp: "20260429060000",
        shortSha: "bbbbbbb",
      }),
      computeVersion("4.10.0", "nightly", {
        timestamp: "20260427060000",
        shortSha: "aaaaaaa",
      }),
      computeVersion("4.10.0", "nightly", {
        timestamp: "20260428060000",
        shortSha: "ccccccc",
      }),
    ];
    const lexSorted = [...versions].sort();
    const semverSorted = [...versions].sort(semver.compare);
    expect(lexSorted).toEqual([
      "4.10.0-pre.20260427060000.nightly.aaaaaaa",
      "4.10.0-pre.20260428060000.nightly.ccccccc",
      "4.10.0-pre.20260429060000.nightly.bbbbbbb",
    ]);
    expect(lexSorted).toEqual(semverSorted);
  });

  it("nightly is a valid semver prerelease", () => {
    const v = computeVersion("4.10.0", "nightly", {
      timestamp: "20260428060000",
      shortSha: "cc66682",
    });
    expect(semver.parse(v)).not.toBeNull();
  });
});

describe("unified pre.* prerelease ordering", () => {
  it("formats main-cut nightly as pre.<ts>.nightly.<sha>", () => {
    expect(
      computeVersion("4.9.0", "nightly", {
        timestamp: "20260825060000",
        shortSha: "a53a97e",
      }),
    ).toBe("4.9.0-pre.20260825060000.nightly.a53a97e");
  });

  it("formats main-cut dev as pre.<ts>.dev.<sha>", () => {
    expect(
      computeVersion("4.9.0", "dev", {
        timestamp: "20260825153000",
        shortSha: "999b077",
        fromMain: true,
      }),
    ).toBe("4.9.0-pre.20260825153000.dev.999b077");
  });

  it("keeps branch-cut dev on the legacy shape", () => {
    expect(computeVersion("4.9.0", "dev", { shortSha: "999b077" })).toBe(
      "4.9.0-dev.999b077",
    );
  });

  it("orders the unified timeline by timestamp across channels", () => {
    const nightly = computeVersion("4.9.0", "nightly", {
      timestamp: "20260825060000",
      shortSha: "a53a97e",
    });
    const dev = computeVersion("4.9.0", "dev", {
      timestamp: "20260825153000",
      shortSha: "999b077",
      fromMain: true,
    });
    expect(semver.gt(dev, nightly)).toBe(true); // later dev beats earlier nightly
  });

  it("orders a later nightly above an earlier dev", () => {
    const dev = computeVersion("4.9.0", "dev", {
      timestamp: "20260825060000",
      shortSha: "999b077",
      fromMain: true,
    });
    const nightly = computeVersion("4.9.0", "nightly", {
      timestamp: "20260825153000",
      shortSha: "a53a97e",
    });
    expect(semver.gt(nightly, dev)).toBe(true);
  });

  it("ignores the timestamp for a branch-cut dev", () => {
    expect(
      computeVersion("4.9.0", "dev", {
        shortSha: "a",
        timestamp: "20260825153000",
      }),
    ).toBe("4.9.0-dev.a");
  });

  it("sorts legacy shapes below all pre.* and rc above", () => {
    const pre = "4.9.0-pre.20260825060000.nightly.a53a97e";
    expect(semver.gt(pre, "4.9.0-nightly.20260826.fffffff")).toBe(true);
    expect(semver.gt(pre, "4.9.0-dev.999b077")).toBe(true);
    expect(semver.gt("4.9.0-rc1", pre)).toBe(true);
  });

  it("getTimestamp returns UTC YYYYMMDDHHMMSS", () => {
    // Pinned to a fixed instant: the stamp must be UTC regardless of the
    // runner's local zone, must carry seconds, and must truncate sub-second
    // precision rather than round.
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-25T15:30:45.987Z"));
    try {
      expect(getTimestamp()).toBe("20260825153045");
    } finally {
      vi.useRealTimers();
    }
  });

  // Regression: with minute precision, two runs starting in the same UTC
  // minute shared an ordering key, so semver fell through to the channel
  // identifier ("dev" < "nightly" alphabetically) and then the sha. A dev cut
  // seconds AFTER a nightly therefore sorted BELOW it — a silent backwards
  // upgrade for anything resolving latest. Second precision separates them.
  it("orders same-minute runs by seconds, not by channel name", () => {
    const nightly = computeVersion("4.9.0", "nightly", {
      timestamp: "20260825060000",
      shortSha: "a53a97e",
    });
    const dev = computeVersion("4.9.0", "dev", {
      timestamp: "20260825060030",
      shortSha: "999b077",
      fromMain: true,
    });
    expect(semver.gt(dev, nightly)).toBe(true);
    // ...and the reverse ordering within the same minute holds too.
    const laterNightly = computeVersion("4.9.0", "nightly", {
      timestamp: "20260825060059",
      shortSha: "ccccccc",
    });
    expect(semver.gt(laterNightly, dev)).toBe(true);
  });

  it("keeps the widened stamp a numeric semver identifier", () => {
    const parsed = semver.parse(
      computeVersion("4.9.0", "nightly", {
        timestamp: "20260825060000",
        shortSha: "a53a97e",
      }),
    );
    // Numeric (not string) => semver compares stamps arithmetically, and the
    // value stays inside Number.MAX_SAFE_INTEGER at 14 digits.
    expect(parsed?.prerelease[1]).toBe(20260825060000);
    expect(Number.isSafeInteger(parsed?.prerelease[1])).toBe(true);
  });

  it("requires timestamp for main-cut builds", () => {
    expect(() =>
      computeVersion("4.9.0", "nightly", { shortSha: "a53a97e" }),
    ).toThrow(/timestamp/);
    expect(() =>
      computeVersion("4.9.0", "dev", { shortSha: "a", fromMain: true }),
    ).toThrow(/timestamp/);
  });
});

describe("validateTimestamp", () => {
  it("passes an absent stamp through as the now-fallback signal", () => {
    expect(validateTimestamp()).toBeUndefined();
    expect(validateTimestamp("")).toBeUndefined();
  });

  it.each([
    ["20260825153045", "a plain instant"],
    ["20240229120000", "a real leap day"],
    ["20261231235959", "the last second of a year"],
    ["20260101000000", "midnight"],
  ])("accepts %s (%s)", (stamp) => {
    expect(validateTimestamp(stamp)).toBe(stamp);
  });

  it.each(["2026", "202601020304", "2026010203040506", "2026082515304a", "  "])(
    "rejects the wrong shape: %s",
    (stamp) => {
      expect(() => validateTimestamp(stamp)).toThrow(/14 digits/);
    },
  );

  // The digit count alone let impossible calendar values through, and those
  // sort by their literal digits — a stamp claiming month 13 lands after every
  // real December and before the following January.
  it.each([
    ["20261301000000", "month 13"],
    ["20260032000000", "month 00"],
    ["20260832000000", "day 32"],
    ["20260800000000", "day 00"],
    ["20260230120000", "30 February"],
    ["20250229120000", "29 February in a non-leap year"],
    ["20260825240000", "hour 24"],
    ["20260825236000", "minute 60"],
    ["20260825235960", "second 60"],
  ])("rejects %s (%s)", (stamp) => {
    expect(() => validateTimestamp(stamp)).toThrow(/real UTC/);
  });

  // The reported case. It is not merely nonsensical: a leading-zero numeric
  // identifier is illegal SemVer, so the version it produces cannot be parsed
  // at all — proven below rather than asserted.
  it("rejects an all-zero stamp, which would emit unparseable semver", () => {
    expect(() => validateTimestamp("00000000000000")).toThrow(/real UTC/);
    expect(semver.parse("4.9.0-pre.00000000000000.dev.999b077")).toBeNull();
  });

  it.each(["09990825153045", "00010101000000"])(
    "rejects the leading-zero stamp %s",
    (stamp) => {
      expect(() => validateTimestamp(stamp)).toThrow(/real UTC/);
    },
  );

  // The mint path (`date -u +%Y%m%d%H%M%S` in release.yml, `getTimestamp()`
  // for standalone runs) must never trip the tightened check.
  it("accepts every stamp getTimestamp mints", () => {
    vi.useFakeTimers();
    try {
      for (const instant of [
        "2026-01-01T00:00:00.000Z",
        "2026-03-01T00:00:00.000Z",
        "2024-02-29T12:00:00.000Z", // leap day
        "2026-12-31T23:59:59.000Z",
        "2026-08-25T15:30:45.987Z",
      ]) {
        vi.setSystemTime(new Date(instant));
        const stamp = getTimestamp();
        expect(validateTimestamp(stamp)).toBe(stamp);
      }
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps every accepted stamp a valid semver numeric identifier", () => {
    const stamp = validateTimestamp("20260825153045");
    const version = computeVersion("4.9.0", "dev", {
      timestamp: stamp,
      shortSha: "999b077",
      fromMain: true,
    });
    expect(semver.parse(version)?.prerelease[1]).toBe(20260825153045);
  });
});
