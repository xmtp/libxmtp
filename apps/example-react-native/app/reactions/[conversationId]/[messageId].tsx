import { useLocalSearchParams } from "expo-router";

import { ReactionsScreen } from "@/features/reactions/ReactionsScreen";

export default function ReactionsRoute() {
  const { conversationId, messageId, emoji } = useLocalSearchParams<{
    conversationId: string;
    messageId: string;
    emoji?: string;
  }>();
  return (
    <ReactionsScreen
      conversationId={conversationId}
      messageId={messageId}
      emoji={emoji ?? ""}
    />
  );
}
