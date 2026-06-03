import { Sdk, type SdkConfig } from "../types";
import {
  createPodspecManifestProvider,
  createGradlePropertiesManifestProvider,
  createCargoManifestProvider,
  createPackageJsonManifestProvider,
} from "./manifest";

export const SDK_CONFIGS: Record<Sdk, SdkConfig> = {
  [Sdk.Ios]: {
    name: "iOS",
    manifestPath: "sdks/ios/XMTP.podspec",
    spmManifestPath: "Package.swift",
    tagPrefix: "ios-",
    artifactTagSuffix: "-libxmtp",
    manifest: createPodspecManifestProvider("sdks/ios/XMTP.podspec"),
    versionTrack: "independent",
    notesIncludeGlobs: ["crates/**", "bindings/mobile/**", "sdks/ios/**"],
    notesExcludeGlobs: [
      "bindings/wasm/**",
      "bindings/node/**",
      "sdks/android/**",
    ],
    releaseWorkflow: "release-ios.yml",
    channels: ["nightly", "rc", "final"],
  },
  [Sdk.Android]: {
    name: "Android",
    manifestPath: "sdks/android/gradle.properties",
    tagPrefix: "android-",
    artifactTagSuffix: "-libxmtp",
    manifest: createGradlePropertiesManifestProvider(
      "sdks/android/gradle.properties",
    ),
    versionTrack: "independent",
    notesIncludeGlobs: ["crates/**", "bindings/mobile/**", "sdks/android/**"],
    notesExcludeGlobs: ["bindings/wasm/**", "bindings/node/**", "sdks/ios/**"],
    releaseWorkflow: "release-android.yml",
    channels: ["nightly", "rc", "final"],
  },
  [Sdk.NodeBindings]: {
    name: "Node",
    manifestPath: "bindings/node/package.json",
    tagPrefix: "node-bindings-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("bindings/node/package.json"),
    versionTrack: "follows-libxmtp",
    notesIncludeGlobs: ["crates/**", "bindings/node/**"],
    notesExcludeGlobs: ["bindings/wasm/**", "bindings/mobile/**"],
    releaseWorkflow: "release-node.yml",
    channels: ["nightly", "rc", "final"],
  },
  [Sdk.WasmBindings]: {
    name: "WASM",
    manifestPath: "bindings/wasm/package.json",
    tagPrefix: "wasm-bindings-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("bindings/wasm/package.json"),
    versionTrack: "follows-libxmtp",
    notesIncludeGlobs: ["crates/**", "bindings/wasm/**"],
    notesExcludeGlobs: ["bindings/node/**", "bindings/mobile/**"],
    releaseWorkflow: "release-wasm.yml",
    channels: ["nightly", "rc", "final"],
  },
  [Sdk.Libxmtp]: {
    name: "Libxmtp",
    manifestPath: "Cargo.toml",
    tagPrefix: "v",
    artifactTagSuffix: "",
    manifest: createCargoManifestProvider("Cargo.toml"),
    versionTrack: "follows-libxmtp",
    notesIncludeGlobs: ["crates/**", "bindings/**"],
    notesExcludeGlobs: [],
    releaseWorkflow: "",
    channels: ["nightly", "rc", "final"],
  },
};

export function getSdkConfig(sdk: string): SdkConfig {
  const config = SDK_CONFIGS[sdk as Sdk];
  if (!config) {
    throw new Error(
      `Unknown SDK: ${sdk}. Available: ${Object.keys(SDK_CONFIGS).join(", ")}`,
    );
  }
  return config;
}
