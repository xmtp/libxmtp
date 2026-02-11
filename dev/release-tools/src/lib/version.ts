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
      return `${normalized}-dev.${options.shortSha}`;
    }
  }
}
