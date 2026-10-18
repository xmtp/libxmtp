import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function DisappearingRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.disappearing}
      route="disappearing/[conversationId]"
    />
  );
}
