import { useLocalSearchParams } from "expo-router";

import { DisappearingScreen } from "@/features/disappearing/DisappearingScreen";

export default function DisappearingRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <DisappearingScreen conversationId={conversationId} />;
}
