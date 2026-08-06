import semver from "semver";
import { diffBumpKind, type PendingRelease } from "./sdk-version";
import type { BumpType } from "../types";

interface CliffRelease {
  version?: string | null;
  bump_type?: BumpType | null;
  previous?: { version?: string | null } | null;
  // git-cliff context carries many more fields; we only read these.
}

const stripV = (v: string): string => v.replace(/^v/, "");

/**
 * Parse `git cliff --bump --context` JSON into the pending libxmtp release.
 * The first (unreleased) entry carries the computed `version` (v-prefixed),
 * an optional `bump_type`, and `previous.version`. When there are no
 * conventional commits past the last tag, git-cliff sets
 * `version == previous.version` with `bump_type = null`; that is "nothing
 * pending" and returns null. Otherwise we use git-cliff's `bump_type` when
 * present, else derive it via `diffBumpKind` against `lastShippedVersion`.
 */
export function parsePendingFromContext(
  json: string,
  lastShippedVersion: string,
): PendingRelease | null {
  let parsed: CliffRelease[];
  try {
    parsed = JSON.parse(json) as CliffRelease[];
  } catch (e) {
    throw new Error(
      `Could not parse git-cliff context: ${(e as Error).message}`,
    );
  }

  if (!Array.isArray(parsed) || parsed.length === 0) return null;

  const entry = parsed[0];
  const raw = entry?.version;
  if (!raw) return null;

  const version = stripV(raw);
  if (!semver.valid(version)) {
    throw new Error(`git-cliff produced an invalid version: "${raw}"`);
  }

  // "Nothing to bump": computed version equals the previous tag.
  const prev = entry.previous?.version ? stripV(entry.previous.version) : null;
  if (prev && semver.eq(version, prev)) return null;

  // Prefer git-cliff's own bump_type; fall back to diffing vs last-shipped.
  const kind: BumpType =
    entry.bump_type ?? diffBumpKind(lastShippedVersion, version);

  return { version, kind };
}
