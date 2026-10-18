import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function DetailsRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.details}
      route="details/[conversationId]"
    />
  );
}
