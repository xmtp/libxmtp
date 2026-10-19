import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function MemberRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.member}
      route="member/[conversationId]/[inboxId]"
    />
  );
}
