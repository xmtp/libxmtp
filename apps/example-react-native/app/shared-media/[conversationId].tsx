import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function SharedMediaRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.sharedMedia}
      route="shared-media/[conversationId]"
    />
  );
}
