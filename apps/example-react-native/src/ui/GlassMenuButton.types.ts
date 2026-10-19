import type { StyleProp, ViewStyle } from "react-native";

import type { SymbolName } from "@/ui/Symbol";

export interface GlassMenuItem {
  readonly title: string;
  readonly systemImage?: SymbolName;
  readonly onPress: () => void;
}

export interface GlassMenuButtonProps {
  /** The glass button's symbol (`.font(.title3)`). */
  readonly systemImage: SymbolName;
  /** Required: the button has no visible title. */
  readonly accessibilityLabel: string;
  /** Menu items, in declaration order (SwiftUI orders them for the edge). */
  readonly items: readonly GlassMenuItem[];
  readonly testID?: string;
  readonly style?: StyleProp<ViewStyle>;
}
