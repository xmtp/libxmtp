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
  [Sdk.NodeBindings]: {
    name: "Node",
    manifestPath: "bindings/node/package.json",
    tagPrefix: "node-bindings-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("bindings/node/package.json"),
  },
  [Sdk.WasmBindings]: {
    name: "WASM",
    manifestPath: "bindings/wasm/package.json",
    tagPrefix: "wasm-bindings-",
    artifactTagSuffix: "",
    manifest: createPackageJsonManifestProvider("bindings/wasm/package.json"),
  },
  [Sdk.Libxmtp]: {
    name: "Libxmtp",
    manifestPath: "Cargo.toml",
    tagPrefix: "v",
    artifactTagSuffix: "",
    manifest: createCargoManifestProvider("Cargo.toml"),
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
