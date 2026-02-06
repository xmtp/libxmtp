import { Sdk, type SdkConfig } from "../types.js";
import { createPodspecManifestProvider } from "./manifest.js";

export const SDK_CONFIGS: Record<Sdk, SdkConfig> = {
  [Sdk.Ios]: {
    name: "iOS",
    manifestPath: "sdks/ios/XMTP.podspec",
    spmManifestPath: "Package.swift",
    tagPrefix: "ios-",
    artifactTagSuffix: "-libxmtp",
    manifest: createPodspecManifestProvider("sdks/ios/XMTP.podspec"),
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
