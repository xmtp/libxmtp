import { Sdk, type SdkConfig } from "../types.js";
import {
  createPodspecManifestProvider,
  createGradlePropertiesManifestProvider,
} from "./manifest.js";

export const SDK_CONFIGS: Record<Sdk, SdkConfig> = {
  [Sdk.Ios]: {
    name: "iOS",
    manifestPath: "sdks/ios/XMTP.podspec",
    spmManifestPath: "Package.swift",
    tagPrefix: "ios-",
    artifactTagSuffix: "-libxmtp",
    manifest: createPodspecManifestProvider("sdks/ios/XMTP.podspec"),
  },
  [Sdk.Android]: {
    name: "Android",
    manifestPath: "sdks/android/gradle.properties",
    tagPrefix: "android-",
    artifactTagSuffix: "-libxmtp",
    manifest: createGradlePropertiesManifestProvider(
      "sdks/android/gradle.properties",
    ),
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
