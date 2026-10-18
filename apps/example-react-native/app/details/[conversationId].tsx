import { useLocalSearchParams } from "expo-router";

import { DetailsScreen } from "@/features/details/DetailsScreen";

export default function DetailsRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <DetailsScreen conversationId={conversationId} />;
}
