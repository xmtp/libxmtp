import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function NewConversationRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.newConversation}
      route="new-conversation/index"
    />
  );
}
