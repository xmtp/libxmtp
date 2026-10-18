import { Sdk, type SdkConfig } from "@/types";

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
  },
  [Sdk.BrowserSdk]: {
    name: "Browser SDK",
    manifestPath: "sdks/browser/package.json",
    tagPrefix: "browser-sdk-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("sdks/browser/package.json"),
    versionTrack: "independent",
  },
  [Sdk.NodeSdk]: {
    name: "Node SDK",
    manifestPath: "sdks/node/package.json",
    tagPrefix: "node-sdk-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("sdks/node/package.json"),
    versionTrack: "independent",
  },
  [Sdk.AgentSdk]: {
    name: "Agent SDK",
    manifestPath: "sdks/agent/package.json",
    tagPrefix: "agent-sdk-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("sdks/agent/package.json"),
    versionTrack: "independent",
  },
  [Sdk.ReactNativeSdk]: {
    name: "React Native",
    manifestPath: "sdks/react-native/package.json",
    tagPrefix: "react-native-sdk-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider(
      "sdks/react-native/package.json",
    ),
    versionTrack: "independent",
  },
  [Sdk.Cli]: {
    name: "CLI",
    manifestPath: "apps/cli/package.json",
    tagPrefix: "cli-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("apps/cli/package.json"),
    versionTrack: "independent",
  },
  [Sdk.Libxmtp]: {
    name: "Libxmtp",
    manifestPath: "Cargo.toml",
    tagPrefix: "v",
    artifactTagSuffix: "",
    manifest: createCargoManifestProvider("Cargo.toml"),
    versionTrack: "follows-libxmtp",
  },
};

export function getSdkConfig(sdk: string): SdkConfig {
  if (!Object.hasOwn(SDK_CONFIGS, sdk)) {
    throw new Error(
      `Unknown SDK: ${sdk}. Available: ${Object.keys(SDK_CONFIGS).join(", ")}`,
    );
  }
  return SDK_CONFIGS[sdk as Sdk];
}
