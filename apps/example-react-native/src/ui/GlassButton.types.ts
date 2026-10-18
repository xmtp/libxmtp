import type { StyleProp, ViewStyle } from "react-native";

import type { SymbolName } from "@/ui/Symbol";

export interface GlassButtonProps {
  title: string;
  onPress: () => void;
  /**
   * `.buttonStyle(.glassProminent)` for primary actions (tint fill, white
   * label) or `.buttonStyle(.glass)` for secondary and destructive ones.
   * Default `glass`.
   */
  variant?: "glass" | "glassProminent";
  /**
   * `destructive` keeps the tint label, like SwiftUI glass buttons on iOS 26.
   * It changes the accessibility role only.
   */
  role?: "destructive" | "cancel";
  /** `.controlSize(.large)` for footer buttons. Default `regular`. */
  size?: "regular" | "large";
  /** The label font. Default `headline` for large, `body` otherwise. */
  font?: "body" | "headline";
  systemImage?: SymbolName;
  /** Stretches to the container width with a centered label. */
  fullWidth?: boolean;
  disabled?: boolean;
  /** Shows a spinner before the title (Connect "Joining server…"). */
  loading?: boolean;
  testID?: string;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
}

export interface GlassIconButtonProps {
  systemImage: SymbolName;
  /** Required: icon-only buttons need a spoken label. */
  accessibilityLabel: string;
  onPress: () => void;
  variant?: "glass" | "glassProminent";
  /** `regular` is a 44 pt circle (toolbar size); `large` is the 48 pt composer size. */
  size?: "regular" | "large";
  disabled?: boolean;
  testID?: string;
  style?: StyleProp<ViewStyle>;
}
