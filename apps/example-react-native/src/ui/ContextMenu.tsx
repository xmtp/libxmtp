import { View } from "react-native";

import type { ContextMenuProps } from "@/ui/ContextMenu.types";

export type { ContextMenuItem, ContextMenuProps } from "@/ui/ContextMenu.types";

/**
 * Android has no context menu container: the children render as they are.
 * Rows offer their menu on long press instead (see `ConversationListRow`).
 */
export function ContextMenu({ children, style }: ContextMenuProps) {
  return <View style={style}>{children}</View>;
}
