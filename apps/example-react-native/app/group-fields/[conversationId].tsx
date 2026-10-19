import { useLocalSearchParams } from "expo-router";

import { MetadataScreen } from "@/features/metadata/MetadataScreen";

export default function GroupFieldsRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <MetadataScreen conversationId={conversationId} own={false} />;
}
