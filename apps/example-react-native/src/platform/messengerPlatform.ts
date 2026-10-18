import { requireNativeModule, requireNativeView } from "expo";
import type { ViewProps } from "react-native";

import {
  type LaunchEnvironment,
  parseLaunchEnvironment,
} from "@/core/launchEnvironment.ts";

/** The local Expo module in `modules/messenger-platform`. */
interface MessengerPlatformModule {
  launchEnvironment(): Record<string, string>;
  applicationSupportDirectory(): string;
  excludeFromBackup(path: string): void;
  protectUntilFirstUserAuthentication(path: string): void;
}

const platform =
  requireNativeModule<MessengerPlatformModule>("MessengerPlatform");

/**
 * Launch settings: iOS process environment (`SIMCTL_CHILD_<NAME>`) or Android
 * launch intent extras (`am start --es <NAME> <value>`). JS cannot read the
 * process environment directly.
 */
export function launchEnvironment(): LaunchEnvironment {
  return parseLaunchEnvironment(platform.launchEnvironment());
}

/** A native path (never a `file://` URI) that exists. */
export function applicationSupportDirectory(): string {
  return platform.applicationSupportDirectory();
}

/** iOS: excludes the item from iCloud and device backups. Android: no-op. */
export function excludeFromBackup(path: string): void {
  platform.excludeFromBackup(path);
}

/** iOS: `completeUntilFirstUserAuthentication`. Android: no-op. */
export function protectUntilFirstUserAuthentication(path: string): void {
  platform.protectUntilFirstUserAuthentication(path);
}

export interface NativeNavigationSubtitleProps extends ViewProps {
  /** The screen's `navigationItem.subtitle` (iOS 26). */
  subtitle?: string;
  /** Shown under the large title; falls back to `subtitle`. */
  largeSubtitle?: string;
}

/**
 * Sets the hosting stack screen's navigation subtitle. It draws nothing;
 * render it anywhere inside the screen. Android renders nothing.
 */
export const NativeNavigationSubtitle =
  requireNativeView<NativeNavigationSubtitleProps>(
    "MessengerPlatform",
    "NavigationSubtitleView",
  );
