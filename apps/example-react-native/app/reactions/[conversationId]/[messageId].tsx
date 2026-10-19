import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function ReactionsRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.reactions}
      route="reactions/[conversationId]/[messageId]"
    />
  );
}
