import type { SdkConfig } from "../types.js";

export const SDK_CONFIGS: Record<string, SdkConfig> = {
  ios: {
    name: "iOS",
    manifestPath: "sdks/ios/XMTP.podspec",
    spmManifestPath: "sdks/ios/Package.swift",
    tagPrefix: "ios-",
    artifactTagSuffix: "-libxmtp",
  },
};

export function getSdkConfig(sdk: string): SdkConfig {
  const config = SDK_CONFIGS[sdk];
  if (!config) {
    throw new Error(
      `Unknown SDK: ${sdk}. Available: ${Object.keys(SDK_CONFIGS).join(", ")}`
    );
  }
  return config;
}
