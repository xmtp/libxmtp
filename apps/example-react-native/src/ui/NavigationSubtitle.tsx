import { StyleSheet } from "react-native";

import { NativeNavigationSubtitle } from "@/platform/messengerPlatform";

export interface NavigationSubtitleProps {
  /** The screen's subtitle (`.navigationSubtitle`), under the title. */
  subtitle: string;
  /** Under the large title. Defaults to `subtitle`. */
  largeSubtitle?: string;
}

/**
 * Sets the current stack screen's navigation subtitle (iOS 26). Render it
 * anywhere inside the screen; it draws nothing. On a pushed or replaced screen
 * the subtitle appears when the transition ends. Android shows no subtitle.
 */
export function NavigationSubtitle({
  subtitle,
  largeSubtitle,
}: NavigationSubtitleProps) {
  return (
    <NativeNavigationSubtitle
      subtitle={subtitle}
      largeSubtitle={largeSubtitle}
      style={styles.hidden}
    />
  );
}

const styles = StyleSheet.create({
  hidden: { height: 0, position: "absolute", width: 0 },
});
