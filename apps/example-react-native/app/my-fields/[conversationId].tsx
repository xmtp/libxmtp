import { routeTitles } from "@/copy/foundation";
import { PlaceholderScreen } from "@/ui/PlaceholderScreen";

export default function MyFieldsRoute() {
  return (
    <PlaceholderScreen
      title={routeTitles.myFields}
      route="my-fields/[conversationId]"
    />
  );
}
