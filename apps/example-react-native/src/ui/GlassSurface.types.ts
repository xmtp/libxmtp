import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export interface GlassSurfaceProps {
  /** `Capsule()` or `RoundedRectangle(cornerRadius:)`. Default `rect`. */
  shape?: "capsule" | "rect";
  /** Corner radius of a `rect` surface. Default 20. */
  cornerRadius?: number;
  /** Glass that reacts to touch (`.glassEffect(.regular.interactive())`). */
  interactive?: boolean;
  style?: StyleProp<ViewStyle>;
  children?: ReactNode;
  testID?: string;
}
