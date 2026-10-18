import type { TurboModule } from "react-native";
import { TurboModuleRegistry } from "react-native";

/** Private native bootstrap ABI; the public SDK is generated from Rust. */
export interface Spec extends TurboModule {
  install(): boolean;
  defaultStorageDirectory(): string;
}

export default TurboModuleRegistry.getEnforcing<Spec>("XmtpReactNative");
