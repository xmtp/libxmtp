import { useLocalSearchParams } from "expo-router";

import { EditGroupScreen } from "@/features/details/EditGroupScreen";

export default function EditGroupRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <EditGroupScreen conversationId={conversationId} />;
}
