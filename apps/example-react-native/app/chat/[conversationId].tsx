import { useLocalSearchParams } from "expo-router";

import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function ChatRoute() {
  const { conversationId } = useLocalSearchParams<{ conversationId: string }>();
  return (
    <PlaceholderScreen
      title={routeTitles.chat}
      route={`chat/${conversationId}`}
    />
  );
}
