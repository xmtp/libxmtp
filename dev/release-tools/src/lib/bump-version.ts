import semver from "semver";

import { getSdkConfig } from "@/lib/sdk-config";
import { normalizeVersion } from "@/lib/version";
import type { BumpType } from "@/types";

export function bumpVersion(
  sdk: string,
  bumpType: BumpType,
  repoRoot: string,
): string {
  const config = getSdkConfig(sdk);
  const currentVersion = config.manifest.readVersion(repoRoot);
  const baseVersion = normalizeVersion(currentVersion);

  const newVersion = semver.inc(baseVersion, bumpType);
  if (!newVersion) {
    throw new Error(`Failed to bump ${bumpType} on version ${baseVersion}`);
  }
  config.manifest.writeVersion(repoRoot, newVersion);
  return newVersion;
}
