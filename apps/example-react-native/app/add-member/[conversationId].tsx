import { useLocalSearchParams } from "expo-router";

import { AddMemberScreen } from "@/features/details/AddMemberScreen";

export default function AddMemberRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <AddMemberScreen conversationId={conversationId} />;
}
