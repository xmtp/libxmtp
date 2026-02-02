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
}

export type ReleaseType = "dev" | "rc" | "final";

export type BumpType = "major" | "minor" | "patch";
