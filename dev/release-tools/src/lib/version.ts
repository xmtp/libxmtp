import semver from "semver";
import type { ReleaseType } from "../types";

/**
 * Normalize a version string by stripping prerelease and build metadata.
 * E.g., "4.9.0-dev.abc123" -> "4.9.0", "4.9.0-rc1" -> "4.9.0"
 */
export function normalizeVersion(version: string): string {
  const parsed = semver.parse(version);
  if (!parsed) {
    throw new Error(`Invalid version format: ${version}`);
  }
  return `${parsed.major}.${parsed.minor}.${parsed.patch}`;
}

/**
 * Filter git tags by SDK prefix, exclude artifact tags, parse as semver,
 * and return sorted version strings (highest first).
 */
export function filterAndSortTags(
  tags: string[],
  prefix: string,
  artifactSuffix: string,
  includePrerelease = false,
): string[] {
  const versions: semver.SemVer[] = [];

  for (const tag of tags) {
    if (!tag.startsWith(prefix)) continue;

    const versionStr = tag.slice(prefix.length);

    // Exclude artifact tags (e.g. ios-4.9.0-libxmtp)
    if (versionStr.endsWith(artifactSuffix)) continue;

    const parsed = semver.parse(versionStr);
    if (!parsed) continue;

    if (!includePrerelease && parsed.prerelease.length > 0) continue;

    versions.push(parsed);
  }

  return versions.sort((a, b) => semver.rcompare(a, b)).map((v) => v.version);
}

export interface ComputeVersionOptions {
  rcNumber?: number;
  shortSha?: string;
  /** UTC YYYYMMDDHHMMSS stamp for the unified pre.* timeline (main-cut builds) */
  timestamp?: string;
  /** True when the release is cut from main; gates the ordered pre.* format */
  fromMain?: boolean;
}

/**
 * Compute the full version string for a given release type.
 */
export function computeVersion(
  baseVersion: string,
  releaseType: ReleaseType,
  options: ComputeVersionOptions = {},
): string {
  const normalized = normalizeVersion(baseVersion);
  switch (releaseType) {
    case "final":
      return normalized;
    case "rc": {
      if (options.rcNumber == null) {
        throw new Error("rcNumber is required for rc releases");
      }
      return `${normalized}-rc${options.rcNumber}`;
    }
    case "dev": {
      if (!options.shortSha) {
        throw new Error("shortSha is required for dev releases");
      }
      if (options.fromMain) {
        if (!options.timestamp) {
          throw new Error("timestamp is required for main-cut dev releases");
        }
        return `${normalized}-pre.${options.timestamp}.dev.${options.shortSha}`;
      }
      // Branch-cut devs keep the legacy, unordered shape: below every pre.*
      // under semver and invisible to consumer renovate configs — delivered
      // only through the bot bump PRs.
      return `${normalized}-dev.${options.shortSha}`;
    }
    case "nightly": {
      if (!options.timestamp) {
        throw new Error("timestamp is required for nightly releases");
      }
      if (!options.shortSha) {
        throw new Error("shortSha is required for nightly releases");
      }
      return `${normalized}-pre.${options.timestamp}.nightly.${options.shortSha}`;
    }
  }
}

/**
 * Now as a compact UTC `YYYYMMDDHHMMSS` stamp for the pre.* timeline.
 *
 * The stamp IS the ordering key: semver compares it as a numeric identifier
 * before it ever looks at the channel or the sha. Two runs that share a stamp
 * therefore fall through to comparing `dev` vs `nightly` (alphabetical, so a
 * newer dev sorts BELOW an older nightly) and then sha (arbitrary) — a
 * backwards "upgrade" for anything resolving latest. Second precision keeps
 * distinct runs on distinct keys; minute precision did not.
 */
export function getTimestamp(): string {
  return formatTimestamp(new Date());
}

/** `YYYYMMDDHHMMSS` for an instant, UTC, truncated (never rounded). */
function formatTimestamp(date: Date): string {
  return date.toISOString().slice(0, 19).replace(/[-T:]/g, "");
}

/**
 * True when the 14 digits name an instant that actually exists.
 *
 * Checked by round trip: every out-of-range component ROLLS OVER through
 * `Date.UTC` (month 13 -> next January, day 32 -> next month, hour 24 -> next
 * day, 30 February -> March), so re-formatting the resulting instant yields
 * different digits than went in. Years below 1000 are rejected up front for two
 * reasons: the stamp is a semver NUMERIC identifier, which must not carry a
 * leading zero (`0…` makes the whole version unparseable), and `Date.UTC` maps
 * years 0-99 onto 1900-1999 rather than the literal year, so the round trip
 * alone would not be trustworthy there.
 */
function isRealUtcStamp(stamp: string): boolean {
  const [year, month, day, hour, minute, second] = [
    stamp.slice(0, 4),
    stamp.slice(4, 6),
    stamp.slice(6, 8),
    stamp.slice(8, 10),
    stamp.slice(10, 12),
    stamp.slice(12, 14),
  ].map(Number);
  if (year < 1000) return false;
  const instant = new Date(
    Date.UTC(year, month - 1, day, hour, minute, second),
  );
  return formatTimestamp(instant) === stamp;
}

/**
 * Validate a caller-supplied `--timestamp`. Every version computed within one
 * Release run must carry the SAME stamp (the consumer bump PRs assert
 * whole-suffix equality across `@xmtp/node-sdk` and `@xmtp/node-bindings`), so
 * the run mints one stamp and threads it into every job.
 *
 * Absent/empty means "no run stamp was threaded" — the caller falls back to
 * `getTimestamp()`, which is correct for standalone invocations. A non-empty
 * but malformed value means the workflow plumbing is broken, so throw rather
 * than fall back and silently desynchronize the run.
 *
 * "Malformed" is more than the wrong digit count. The stamp lands verbatim in a
 * semver prerelease identifier and is the ordering key, so it must also name a
 * real UTC instant: `00000000000000` passes a digits-only test yet yields
 * `X.Y.Z-pre.00000000000000.<channel>.<sha>`, which semver refuses to parse at
 * all (numeric identifiers may not have a leading zero), and `20261301…` parses
 * happily while sorting nowhere near where its digits claim. The emitted format
 * is unchanged — `date -u +%Y%m%d%H%M%S` and `getTimestamp()` only ever produce
 * stamps that pass — this only rejects values no minting path can produce.
 */
export function validateTimestamp(supplied?: string): string | undefined {
  if (!supplied) return undefined;
  if (!/^\d{14}$/.test(supplied)) {
    throw new Error(
      `--timestamp must be a UTC YYYYMMDDHHMMSS stamp (14 digits), got: ${supplied}`,
    );
  }
  if (!isRealUtcStamp(supplied)) {
    throw new Error(
      `--timestamp must be a real UTC YYYYMMDDHHMMSS instant, got: ${supplied}`,
    );
  }
  return supplied;
}
