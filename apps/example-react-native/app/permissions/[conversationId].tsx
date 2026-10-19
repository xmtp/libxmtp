import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function PermissionsRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.permissions}
      route="permissions/[conversationId]"
    />
  );
}
