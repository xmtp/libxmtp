import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function GroupFieldsRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.groupFields}
      route="group-fields/[conversationId]"
    />
  );
}
