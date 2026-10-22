import { useLocalSearchParams } from "expo-router";

import { MetadataScreen } from "@/features/metadata/MetadataScreen";

export default function MyFieldsRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <MetadataScreen conversationId={conversationId} own />;
}
