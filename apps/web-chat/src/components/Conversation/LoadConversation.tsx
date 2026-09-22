import { LoadingOverlay } from "@mantine/core";
import { useEffect } from "react";
import { useNavigate, useParams } from "react-router";

import { CenteredLayout } from "@/layouts/CenteredLayout";
import { useActions, useLastSyncedAt } from "@/stores/inbox/hooks";

import { Conversation } from "./Conversation";

export const LoadConversation: React.FC = () => {
  const navigate = useNavigate();
  const { conversationId } = useParams();
  const lastSyncedAt = useLastSyncedAt();
  const { getConversation } = useActions();
  const conversation =
    lastSyncedAt && conversationId
      ? getConversation(conversationId)
      : undefined;

  useEffect(() => {
    // wait for initial sync to complete
    if (lastSyncedAt && conversationId) {
      if (!conversation) {
        void navigate(`/conversations`);
      }
    }
  }, [conversation, conversationId, lastSyncedAt, navigate]);

  return conversation ? (
    <Conversation conversationId={conversation.id} />
  ) : (
    <CenteredLayout>
      <LoadingOverlay visible />
    </CenteredLayout>
  );
};
