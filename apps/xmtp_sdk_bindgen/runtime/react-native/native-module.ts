import { TurboModuleRegistry, type TurboModule } from "react-native";

interface NativeHost extends TurboModule {
  install(): boolean;
  defaultStorageDirectory(): string;
}

const host = TurboModuleRegistry.getEnforcing<NativeHost>("XmtpReactNative");
if (!host.install()) {
  throw new Error("XMTP native runtime installation failed");
}

export function defaultStorageDirectory(): string {
  return host.defaultStorageDirectory();
}
