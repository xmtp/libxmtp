import { useLocalSearchParams } from "expo-router";

import { PermissionsScreen } from "@/features/permissions/PermissionsScreen";

export default function PermissionsRoute() {
  const { conversationId } = useLocalSearchParams<{
    conversationId: string;
  }>();
  return <PermissionsScreen conversationId={conversationId} />;
}
