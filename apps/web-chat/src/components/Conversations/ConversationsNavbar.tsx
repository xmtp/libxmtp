import { Badge, Box, Group, Stack, Text } from "@mantine/core";
import { useCallback, useEffect, useMemo } from "react";

import { ConversationsList } from "@/components/Conversations/ConversationList";
import { ConversationsMenu } from "@/components/Conversations/ConversationsMenu";
import { HelpCard } from "@/components/Conversations/HelpCard";
import { createStreamSession } from "@/helpers/streamSession";
import { useConversations } from "@/hooks/useConversations";
import { useHelpDm } from "@/hooks/useHelpDm";
import { ContentLayout } from "@/layouts/ContentLayout";

export const ConversationsNavbar: React.FC = () => {
  const {
    sync,
    loading,
    syncing,
    conversations,
    stream,
    streamAllMessages,
    syncAll,
  } = useConversations();
  const { exists: helpDmExists } = useHelpDm();
  const streamSession = useMemo(
    () => createStreamSession([stream, streamAllMessages]),
    [stream, streamAllMessages],
  );

  const handleSync = useCallback(async () => {
    await streamSession.start(() => sync());
  }, [streamSession, sync]);

  const handleSyncAll = useCallback(async () => {
    await streamSession.start(syncAll);
  }, [streamSession, syncAll]);

  // The same session owns initial setup and user-requested refreshes.
  useEffect(() => {
    void streamSession.start(() => sync(true));
    return streamSession.stop;
  }, [streamSession, sync]);

  return (
    <ContentLayout
      withBorders={false}
      title={
        <Group align="center" gap="xs">
          <Text size="md" fw={700}>
            Conversations
          </Text>
          <Badge color="gray" size="lg">
            {conversations.length}
          </Badge>
        </Group>
      }
      loading={conversations.length === 0 && loading}
      headerActions={
        <ConversationsMenu
          loading={syncing || loading}
          onSync={() => void handleSync()}
          onSyncAll={() => void handleSyncAll()}
          disabled={syncing}
        />
      }
      withScrollArea={false}>
      <Stack gap={0} style={{ flexGrow: 1, minHeight: 0 }}>
        {!helpDmExists && <HelpCard />}
        {conversations.length === 0 ? (
          <Box
            display="flex"
            style={{
              flexGrow: 1,
              alignItems: "center",
              justifyContent: "center",
            }}>
            <Text>No conversations found</Text>
          </Box>
        ) : (
          <ConversationsList conversations={conversations} />
        )}
      </Stack>
    </ContentLayout>
  );
};
