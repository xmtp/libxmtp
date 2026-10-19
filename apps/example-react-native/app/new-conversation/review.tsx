import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function NewConversationReviewRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.newConversation}
      route="new-conversation/review"
    />
  );
}
