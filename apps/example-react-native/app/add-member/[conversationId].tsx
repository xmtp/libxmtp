import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function AddMemberRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.addMember}
      route="add-member/[conversationId]"
    />
  );
}
