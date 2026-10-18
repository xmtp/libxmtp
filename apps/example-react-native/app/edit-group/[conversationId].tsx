import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function EditGroupRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.editGroup}
      route="edit-group/[conversationId]"
    />
  );
}
