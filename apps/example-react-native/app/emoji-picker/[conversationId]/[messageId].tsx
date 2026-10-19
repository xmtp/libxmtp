import { useLocalSearchParams } from "expo-router";

import { EmojiPickerScreen } from "@/features/emoji/EmojiPickerScreen";

export default function EmojiPickerRoute() {
  const { conversationId, messageId } = useLocalSearchParams<{
    conversationId: string;
    messageId: string;
  }>();
  return (
    <EmojiPickerScreen conversationId={conversationId} messageId={messageId} />
  );
}
