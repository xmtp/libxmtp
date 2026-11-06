// SwiftUI context-menu previews do not deliver taps to their emoji buttons.
// Keep quick reactions in the same actionable overlay as the attachment menu.
export {
  MessageContextMenu,
  type MessageContextMenuProps,
} from "@/features/chat/interactions/MessageContextMenuContent";
