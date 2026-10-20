import { useLocalSearchParams } from "expo-router";

import { MemberScreen } from "@/features/member/MemberScreen";

export default function MemberRoute() {
  const { conversationId, inboxId } = useLocalSearchParams<{
    conversationId: string;
    inboxId: string;
  }>();
  return <MemberScreen conversationId={conversationId} inboxId={inboxId} />;
}
