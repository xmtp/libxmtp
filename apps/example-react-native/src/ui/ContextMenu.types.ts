import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export interface ContextMenuItem {
  readonly id: string;
  readonly title: string;
  readonly destructive?: boolean;
}

export interface ContextMenuProps {
  /** Menu items, top to bottom. No items: no menu. */
  items: readonly ContextMenuItem[];
  onSelect: (id: string) => void;
  /**
   * The lifted content's background in dark mode. `page` (default) keeps
   * the page color, like a SwiftUI row with `.listRowBackground(Color.clear)`
   * (Chats). `elevated` uses the system's elevated color, like a row with the
   * default background (Message requests).
   */
  liftedBackground?: "page" | "elevated";
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
}
