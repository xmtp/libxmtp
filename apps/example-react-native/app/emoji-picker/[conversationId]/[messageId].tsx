import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function EmojiPickerRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.emojiPicker}
      route="emoji-picker/[conversationId]/[messageId]"
    />
  );
}
