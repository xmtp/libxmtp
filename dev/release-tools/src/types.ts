export enum Sdk {
  Ios = "ios",
  Android = "android",
  Libxmtp = "libxmtp",
}

export interface ManifestProvider {
  readVersion(repoRoot: string): string;
  writeVersion(repoRoot: string, version: string): void;
}

export interface SdkConfig {
  /** Human-readable SDK name */
  name: string;
  /** Path to the version manifest file, relative to repo root */
  manifestPath: string;
  /** Path to SPM Package.swift, relative to repo root (optional, iOS-specific) */
  spmManifestPath?: string;
  /** Git tag prefix for this SDK */
  tagPrefix: string;
  /** Suffix for intermediate artifact tags */
  artifactTagSuffix: string;
  /** Provider for reading/writing the version manifest */
  manifest: ManifestProvider;
}

export type ReleaseType = "dev" | "rc" | "final";

export type BumpType = "major" | "minor" | "patch";

export type BumpOption = BumpType | "none";

/** Valid bump options for CLI commands */
export const BUMP_OPTIONS = ["major", "minor", "patch", "none"] as const;

/** Shape of the global CLI options (defined in cli.ts) */
export interface GlobalArgs {
  repoRoot: string;
}
