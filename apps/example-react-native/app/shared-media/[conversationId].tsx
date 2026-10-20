import { useLocalSearchParams } from "expo-router";

import { SharedMediaScreen } from "@/features/attachments/SharedMediaScreen";

export default function SharedMediaRoute() {
  const { conversationId } = useLocalSearchParams<{ conversationId: string }>();
  return <SharedMediaScreen conversationId={conversationId} />;
}
