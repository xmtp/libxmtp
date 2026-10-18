import { requireNativeView } from "expo";
import type { NativeSyntheticEvent, ViewProps } from "react-native";

import type { ContextMenuProps } from "@/ui/ContextMenu.types";

export type { ContextMenuItem, ContextMenuProps } from "@/ui/ContextMenu.types";

interface NativeContextMenuProps extends ViewProps {
  items: readonly { id: string; title: string; destructive: boolean }[];
  onSelect: (event: NativeSyntheticEvent<{ id: string }>) => void;
}

const NativeContextMenu = requireNativeView<NativeContextMenuProps>(
  "MessengerPlatform",
  "ContextMenuView",
);

/**
 * A UIKit context menu on its children (SwiftUI `.contextMenu` on a list
 * row): a long press lifts the content and shows the items. Children keep
 * their touches and accessibility.
 */
export function ContextMenu({
  items,
  onSelect,
  children,
  style,
}: ContextMenuProps) {
  return (
    <NativeContextMenu
      style={style}
      items={items.map((item) => ({
        id: item.id,
        title: item.title,
        destructive: item.destructive ?? false,
      }))}
      onSelect={(event) => {
        onSelect(event.nativeEvent.id);
      }}>
      {children}
    </NativeContextMenu>
  );
}
