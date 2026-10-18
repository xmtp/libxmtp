import type { StyleProp, ViewStyle } from "react-native";

import type { ColorInput } from "@/ui/colors";
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
  font?: "body" | "headline" | "subheadline";
  /**
   * The label color of a `glass` button when a parent sets
   * `.foregroundStyle` (Connect's red status Retry). Default the tint.
   */
  labelColor?: ColorInput;
  systemImage?: SymbolName;
  /**
   * A symbol at the trailing edge of a full-width button, after the leading
   * title (`HStack { Text; Spacer(); Image }`, the request bar's "Sender
   * details ›").
   */
  trailingSystemImage?: SymbolName;
  /**
   * With `fullWidth`: the title itself fills the width
   * (`Text(title).frame(maxWidth: .infinity)`) instead of sitting between two
   * Spacers, so a long title in a narrow button keeps one line (the request
   * bar's "Delete request").
   */
  labelFills?: boolean;
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
  /**
   * `circle` (`.buttonBorderShape(.circle)`, default), or `capsule`: the
   * glass styles' own shape with no border shape, about 42 x 32 pt (the
   * composer's Send and Cancel reply).
   */
  shape?: "circle" | "capsule";
  disabled?: boolean;
  testID?: string;
  style?: StyleProp<ViewStyle>;
}
