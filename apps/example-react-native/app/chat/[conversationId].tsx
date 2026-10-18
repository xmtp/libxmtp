import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function ChatRoute() {
  return (
    <PlaceholderScreen title={routeTitles.chat} route="chat/[conversationId]" />
  );
}
