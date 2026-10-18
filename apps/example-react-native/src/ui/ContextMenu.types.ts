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
  children?: ReactNode;
  style?: StyleProp<ViewStyle>;
}
