import semver from "semver";
import type { BumpType, ReleaseType, VersionTrack } from "../types";
import { computeVersion } from "./version";

/** The pending libxmtp release: number from the git-cliff oracle, kind derived. */
export interface PendingRelease {
  version: string; // e.g. "1.11.0"
  kind: BumpType; // "major" | "minor" | "patch"
}

/** Determine the bump kind between a base and a target version. */
export function diffBumpKind(from: string, to: string): BumpType {
  const a = semver.parse(from);
  const b = semver.parse(to);
  if (!a || !b) {
    throw new Error(`Invalid version(s): from="${from}" to="${to}"`);
  }
  if (semver.compare(b, a) <= 0) {
    throw new Error(`Target ${to} must be greater than base ${from}`);
  }
  if (b.major > a.major) return "major";
  if (b.minor > a.minor) return "minor";
  return "patch";
}

/** Apply a bump kind to a base version, returning the bumped base. */
export function applyBumpKind(base: string, kind: BumpType): string {
  const bumped = semver.inc(base, kind);
  if (!bumped) {
    throw new Error(`Could not apply ${kind} bump to ${base}`);
  }
  return bumped;
}

export interface ResolveSdkVersionArgs {
  track: VersionTrack;
  /** The SDK's own current base version (from its manifest). */
  base: string;
  /** The pending libxmtp release (git-cliff oracle number + derived kind). */
  pending: PendingRelease;
  releaseType: ReleaseType;
  rcNumber?: number;
  nightlyDate?: string;
  shortSha?: string;
}

/**
 * Resolve the version string for an SDK given its track and the pending
 * libxmtp release. follows-libxmtp uses the pending number directly;
 * independent mirrors the pending bump KIND onto the SDK's own base.
 * Prereleases (nightly/rc/dev) are built on top of the resolved target so
 * they preview the next version (semver-correct ordering).
 */
export function resolveSdkVersion(args: ResolveSdkVersionArgs): string {
  let target: string;
  switch (args.track) {
    case "follows-libxmtp":
      target = args.pending.version;
      break;
    case "independent":
      target = applyBumpKind(args.base, args.pending.kind);
      break;
    default: {
      // Exhaustiveness: adding a new VersionTrack without a branch here is a
      // compile error rather than a silent fall-through.
      const _exhaustive: never = args.track;
      throw new Error(`Unhandled version track: ${String(_exhaustive)}`);
    }
  }

  return computeVersion(target, args.releaseType, {
    rcNumber: args.rcNumber,
    nightlyDate: args.nightlyDate,
    shortSha: args.shortSha,
  });
}
