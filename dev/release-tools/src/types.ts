export enum Sdk {
  Ios = "ios",
  Android = "android",
  NodeBindings = "node-bindings",
  WasmBindings = "wasm-bindings",
  BrowserSdk = "browser-sdk",
  NodeSdk = "node-sdk",
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
  /** How this SDK's version relates to the libxmtp 1.x version */
  versionTrack: VersionTrack;
  /** Code paths whose changes count toward this SDK's filtered notes */
  notesIncludeGlobs: string[];
  /** Code paths excluded from this SDK's filtered notes */
  notesExcludeGlobs: string[];
  /** Reusable workflow file the hub fans out to for this SDK */
  releaseWorkflow: string;
  /** Channels this SDK ships on */
  channels: Channel[];
}

export type ReleaseType = "dev" | "rc" | "final" | "nightly";

export type BumpType = "major" | "minor" | "patch";

export type BumpOption = BumpType | "none";

/** Valid bump options for CLI commands */
export const BUMP_OPTIONS = ["major", "minor", "patch", "none"] as const;

/**
 * How an SDK's version relates to the libxmtp (1.x) version computed from commits.
 * - follows-libxmtp: take the 1.x oracle number verbatim (node/wasm bindings)
 * - independent: own base version; mirror the 1.x bump KIND onto it (iOS/Android)
 */
export type VersionTrack = "follows-libxmtp" | "independent";

export type Channel = "nightly" | "rc" | "final";

/** Shape of the global CLI options (defined in cli.ts) */
export interface GlobalArgs {
  repoRoot: string;
}
